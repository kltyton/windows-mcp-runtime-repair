use std::{marker::PhantomData, rc::Rc};

use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            TaskScheduler::{ITaskService, TaskScheduler},
            Variant::VARIANT,
        },
    },
    core::{BSTR, HRESULT},
};

struct Apartment(PhantomData<Rc<()>>);

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: this non-Send guard balances one successful CoInitializeEx
        // on the same thread, after all local COM interfaces have been released.
        unsafe { CoUninitialize() };
    }
}

pub(super) fn xml(name: &str) -> Result<Option<String>> {
    // SAFETY: no reserved pointer is supplied; the guard balances initialization.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .context("Initialize the task registration interface")?;
    let _apartment = Apartment(PhantomData);
    // SAFETY: COM is initialized on this thread, the class and requested interface
    // are the documented Task Scheduler pair, and aggregation is not requested.
    let service: ITaskService =
        unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }
            .context("Open the local Task Scheduler interface")?;
    let empty = VARIANT::default();
    // SAFETY: all four VT_EMPTY variants stay alive for the synchronous call;
    // the documented empty server and identity select the local current token.
    unsafe { service.Connect(&empty, &empty, &empty, &empty) }
        .context("Connect to local task registration")?;
    // SAFETY: the service is connected and the owned BSTR remains live for the call.
    let folder = unsafe { service.GetFolder(&BSTR::from("\\")) }?;
    // SAFETY: the folder is live and the owned task name remains live for the call.
    let task = match unsafe { folder.GetTask(&BSTR::from(name)) } {
        Ok(task) => task,
        Err(error) if error.code() == HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0) => {
            return Ok(None);
        }
        Err(error) => return Err(error).context("Read the owned task registration"),
    };
    // SAFETY: the registered task interface is live in this apartment; the returned
    // BSTR owns the Unicode XML and is converted before apartment teardown.
    let xml = unsafe { task.Xml() }?;
    Ok(Some(
        String::try_from(xml).context("Task registration XML is not valid UTF-16")?,
    ))
}
