param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$UpstreamRoot,
    [Parameter(Mandatory)][string]$BackendVersion,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$TemporaryDirectory,
    [string]$DiagnosticPrivatePrefix,
    [string]$DiagnosticPublicPrefix
)
$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
[void][System.Management.Automation.SemanticVersion]::new($BackendVersion)
$commit = (& git -C $UpstreamRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $commit -notmatch '^[0-9a-f]{40}$') { throw 'Upstream commit identity is unavailable.' }
[void][IO.Directory]::CreateDirectory($OutputDirectory)
[void][IO.Directory]::CreateDirectory($TemporaryDirectory)
$stage = Join-Path $TemporaryDirectory ([Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($stage)
$archive = Join-Path $OutputDirectory "windows-runtime-$BackendVersion.zip"
if (Test-Path -LiteralPath $archive) { throw 'The payload archive already exists; it was not replaced.' }
try {
    [IO.File]::Copy($Executable, (Join-Path $stage 'codex.exe'))
    if($DiagnosticPrivatePrefix) {
        $normalized=& (Join-Path $PSScriptRoot 'Normalize-NativeDiagnostics.ps1') -Executable (Join-Path $stage 'codex.exe') -PrivatePrefix $DiagnosticPrivatePrefix -PublicPrefix $DiagnosticPublicPrefix
        Write-Host "Normalized $normalized native diagnostic source prefixes."
    }
    foreach ($name in @('LICENSE','NOTICE')) { [IO.File]::Copy((Join-Path $UpstreamRoot $name), (Join-Path $stage $name)) }
    & (Join-Path $PSScriptRoot 'Write-DependencyNotices.ps1') -ManifestPath (Join-Path $UpstreamRoot 'codex-rs\Cargo.toml') -RootPackage 'codex-cli' -Destination (Join-Path $stage 'THIRD_PARTY_NOTICES.txt')
    Compress-Archive -LiteralPath @((Join-Path $stage 'codex.exe'),(Join-Path $stage 'LICENSE'),(Join-Path $stage 'NOTICE'),(Join-Path $stage 'THIRD_PARTY_NOTICES.txt')) -DestinationPath $archive -CompressionLevel Optimal
    [pscustomobject]@{
        backend_version = $BackendVersion
        source_commit = $commit
        archive_url = "https://github.com/kltyton/windows-mcp-runtime-repair/releases/download/backend-v$BackendVersion/windows-runtime-$BackendVersion.zip"
        archive_sha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        executable_sha256 = (Get-FileHash -LiteralPath (Join-Path $stage 'codex.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    }
} finally {
    foreach ($name in @('codex.exe','LICENSE','NOTICE','THIRD_PARTY_NOTICES.txt')) { [IO.File]::Delete((Join-Path $stage $name)) }
    [IO.Directory]::Delete($stage,$false)
}
