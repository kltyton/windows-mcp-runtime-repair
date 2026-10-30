use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use quick_xml::events::Event;

mod registration;

const TASK_NAME: &str = r"\McpRuntimeRepair-PrepareAfterDeployment";

fn system_tool(name: &str) -> Result<std::path::PathBuf> {
    Ok(
        std::path::PathBuf::from(std::env::var_os("SystemRoot").context("SystemRoot is not set")?)
            .join("System32")
            .join(name),
    )
}

fn task_command() -> Result<Option<String>> {
    let Some(text) = registration::xml(TASK_NAME)? else {
        return Ok(None);
    };
    let mut reader = quick_xml::Reader::from_str(&text);
    loop {
        match reader.read_event()? {
            Event::Start(event) if event.local_name().as_ref() == b"Command" => {
                let text = reader.read_text(event.name())?;
                return Ok(Some(quick_xml::escape::unescape(&text)?.into_owned()));
            }
            Event::Eof => bail!("Registered update task has no executable action"),
            _ => {}
        }
    }
}

fn current_sid() -> Result<String> {
    let output = Command::new(system_tool("whoami.exe")?)
        .args(["/user", "/fo", "csv", "/nh"])
        .creation_flags(0x0800_0000)
        .output()?;
    ensure!(
        output.status.success(),
        "Could not identify the current task principal"
    );
    let mut csv = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(output.stdout.as_slice());
    let record = csv
        .byte_records()
        .next()
        .transpose()?
        .context("Current identity response is empty")?;
    let sid = std::str::from_utf8(record.get(1).context("Current identity has no SID")?)?;
    ensure!(
        sid.starts_with("S-1-"),
        "Current identity returned an invalid SID"
    );
    Ok(sid.to_owned())
}

pub fn register(host: &Path, temporary_root: &Path, previous_host: Option<&Path>) -> Result<()> {
    let existing = task_command()?;
    if let Some(command) = &existing {
        ensure!(
            previous_host == Some(Path::new(command)),
            "A different program owns the update task; it was not replaced"
        );
    }
    let host_text = host.to_str().context("Update host path is not Unicode")?;
    let executable = quick_xml::escape::escape(host_text);
    let sid_value = current_sid()?;
    let sid = quick_xml::escape::escape(&sid_value);
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
 <RegistrationInfo><Author>McpRuntimeRepair</Author><Description>Prepare matching runtime after application deployment.</Description></RegistrationInfo>
 <Triggers><EventTrigger><Enabled>true</Enabled>
  <Subscription>&lt;QueryList&gt;&lt;Query Id="0" Path="Microsoft-Windows-AppXDeploymentServer/Operational"&gt;&lt;Select Path="Microsoft-Windows-AppXDeploymentServer/Operational"&gt;*[System[(EventID=400)]]&lt;/Select&gt;&lt;/Query&gt;&lt;/QueryList&gt;</Subscription>
  <ValueQueries><Value name="PackageFullName">Event/EventData/Data[@Name='PackageFullName']</Value></ValueQueries>
 </EventTrigger></Triggers>
 <Principals><Principal id="User"><UserId>{sid}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
 <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><ExecutionTimeLimit>PT30M</ExecutionTimeLimit><RestartOnFailure><Interval>PT10M</Interval><Count>3</Count></RestartOnFailure></Settings>
 <Actions Context="User"><Exec><Command>{executable}</Command><Arguments>--prepare-after-deployment "$(PackageFullName)"</Arguments></Exec></Actions>
</Task>"#
    );
    std::fs::create_dir_all(temporary_root)?;
    let mut file = tempfile::Builder::new()
        .suffix(".xml")
        .tempfile_in(temporary_root)?;
    let xml_bytes: Vec<u8> = std::iter::once(0xfeff_u16)
        .chain(xml.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    file.write_all(&xml_bytes)?;
    file.flush()?;
    let path = file.into_temp_path();
    let mut command = Command::new(system_tool("schtasks.exe")?);
    command
        .args(["/Create", "/TN", TASK_NAME, "/XML"])
        .arg(path.as_os_str());
    if existing.is_some() {
        command.arg("/F");
    }
    let output = command.creation_flags(0x0800_0000).output()?;
    ensure!(
        output.status.success(),
        "Could not register deployment preparation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    path.close()?;
    Ok(())
}

pub fn unregister(host: &Path) -> Result<()> {
    let command =
        task_command()?.context("The owned update task is unavailable; no task was deleted")?;
    ensure!(
        Path::new(&command) == host,
        "Another program owns the update task; it was not deleted"
    );
    let status = Command::new(system_tool("schtasks.exe")?)
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .creation_flags(0x0800_0000)
        .status()?;
    ensure!(status.success(), "Could not remove the owned update task");
    Ok(())
}
