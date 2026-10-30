param(
    [Parameter(Mandatory)][string]$BinaryDirectory,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$TemporaryDirectory,
    [string]$Version='0.1.0'
)
$ErrorActionPreference='Stop'
[Console]::InputEncoding=[Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$OutputEncoding=[Console]::OutputEncoding
[void][System.Management.Automation.SemanticVersion]::new($Version)
$project=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
[void][IO.Directory]::CreateDirectory($TemporaryDirectory)
[void][IO.Directory]::CreateDirectory($OutputDirectory)
$stage=Join-Path $TemporaryDirectory ([Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($stage)
$archive=Join-Path $OutputDirectory "windows-mcp-runtime-repair-$Version-x64.zip"
if(Test-Path -LiteralPath $archive) {throw 'The tool archive already exists; it was not replaced.'}
$names=@('mcp-runtime-repair.exe','runtime-host.exe','README.md','LICENSE','NOTICE','THIRD_PARTY_NOTICES.txt')
try {
    foreach($name in @('mcp-runtime-repair.exe','runtime-host.exe')) {[IO.File]::Copy((Join-Path $BinaryDirectory $name),(Join-Path $stage $name))}
    foreach($name in @('README.md','LICENSE','NOTICE')) {[IO.File]::Copy((Join-Path $project $name),(Join-Path $stage $name))}
    & (Join-Path $PSScriptRoot 'Write-DependencyNotices.ps1') -ManifestPath (Join-Path $project 'Cargo.toml') -RootPackage 'mcp-runtime-repair' -Destination (Join-Path $stage 'THIRD_PARTY_NOTICES.txt')
    Compress-Archive -LiteralPath @($names | ForEach-Object {Join-Path $stage $_}) -DestinationPath $archive -CompressionLevel Optimal
    [pscustomobject]@{path=$archive;sha256=(Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()}
} finally {
    foreach($name in $names) {[IO.File]::Delete((Join-Path $stage $name))}
    [IO.Directory]::Delete($stage,$false)
}
