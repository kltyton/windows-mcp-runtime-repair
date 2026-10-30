param(
    [Parameter(Mandatory)][string]$BackendVersion,
    [Parameter(Mandatory)][string]$WorkingDirectory,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
[void][System.Management.Automation.SemanticVersion]::new($BackendVersion)
$project = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
[void][IO.Directory]::CreateDirectory($WorkingDirectory)
$upstream = Join-Path $WorkingDirectory 'upstream'
if (Test-Path -LiteralPath $upstream) { throw 'The upstream build directory already exists; existing work was not overwritten.' }
function Invoke-Checked([string]$Program,[string[]]$Arguments,[string]$Directory) {
    Push-Location -LiteralPath $Directory
    try {
        & $Program @Arguments | ForEach-Object { Write-Host $_ }
        if ($LASTEXITCODE -ne 0) { throw "$Program exited $LASTEXITCODE" }
    } finally { Pop-Location }
}
function Get-RegistryDependencyGraph([string]$Lockfile) {
    $parser = 'import sys,json,tomllib;from pathlib import Path;v=tomllib.loads(Path(sys.argv[1]).read_text(encoding="utf-8"));print(json.dumps(sorted((p["name"],p["version"],p["source"]) for p in v["package"] if "source" in p)))'
    $result = & python -I -B -c $parser $Lockfile
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the locked upstream dependency graph.' }
    return $result
}
Invoke-Checked git @('-c','core.longpaths=true','-c','core.autocrlf=true','clone','--depth','1','--branch',"rust-v$BackendVersion",'https://github.com/openai/codex.git',$upstream) $WorkingDirectory
$lockfile = Join-Path $upstream 'codex-rs\Cargo.lock'
$before = Get-RegistryDependencyGraph $lockfile
$patchName=if($BackendVersion -eq '0.163.0-alpha.10.2') {'windows-stdio-runtime-legacy.patch'} else {'windows-stdio-runtime.patch'}
Invoke-Checked git @('-c','core.longpaths=true','apply','--whitespace=nowarn',(Join-Path $project ('patches\'+$patchName))) $upstream
$toolchainParser = 'import sys,tomllib;from pathlib import Path;print(tomllib.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))["toolchain"]["channel"])'
$toolchainFile = Join-Path $upstream 'rust-toolchain.toml'
if (!(Test-Path -LiteralPath $toolchainFile)) { $toolchainFile = Join-Path $upstream 'codex-rs\rust-toolchain.toml' }
$channel = (& python -I -B -c $toolchainParser $toolchainFile).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Could not read the matching upstream Rust toolchain.' }
Invoke-Checked rustup @('toolchain','install',$channel,'--profile','minimal','--no-self-update') $WorkingDirectory
$env:RUSTUP_TOOLCHAIN = $channel
$env:CARGO_TARGET_DIR = Join-Path $WorkingDirectory 'target'
$env:TEMP = Join-Path $WorkingDirectory 'temporary'
[void][IO.Directory]::CreateDirectory($env:TEMP)
$env:TMP=$env:TEMP; $env:TMPDIR=$env:TEMP
$env:CARGO_ENCODED_RUSTFLAGS='--remap-path-prefix='+[IO.Path]::GetFullPath($WorkingDirectory)+'=build'+[char]31+'--remap-path-prefix='+$env:USERPROFILE+'=user'
$cargoRoot = Join-Path $upstream 'codex-rs'
# Cargo owns the workspace-version stamping update; external dependency versions stay fixed.
Invoke-Checked cargo @('build','--release','-p','codex-cli','--bin','codex') $cargoRoot
$after = Get-RegistryDependencyGraph $lockfile
if ($before -cne $after) { throw 'The build changed upstream external dependency versions; no payload was released.' }
Invoke-Checked cargo @('build','--release','--locked','-p','codex-rmcp-client','--bin','test_stdio_server') $cargoRoot
$test = 'local_stdio_transport::windows_tests::idle_stdio_reads_leave_blocking_pool_available_for_config_io'
$testLog = Join-Path $WorkingDirectory 'native-regression.log'
Push-Location -LiteralPath $cargoRoot
try {
    & cargo test --release --locked -p codex-rmcp-client --lib $test -- --exact --nocapture *> $testLog
    if ($LASTEXITCODE -ne 0) { throw 'The native pipe regression failed.' }
} finally { Pop-Location }
$text = [IO.File]::ReadAllText($testLog)
if ($text -notmatch 'test result: ok\. 1 passed; 0 failed;' -or $text -notmatch [regex]::Escape("test $test ... ok")) { throw 'The expected native regression did not actually pass.' }
& (Join-Path $project 'tools\packaging\New-RuntimeArchive.ps1') -Executable (Join-Path $env:CARGO_TARGET_DIR 'release\codex.exe') -UpstreamRoot $upstream -BackendVersion $BackendVersion -OutputDirectory $OutputDirectory -TemporaryDirectory $env:TEMP
