param([Parameter(Mandatory)][string]$WorkingDirectory)
$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
$uri = 'https://persistent.oaistatic.com/codex-app-prod/ChatGPT-x64.msix'
[void][IO.Directory]::CreateDirectory($WorkingDirectory)
$packageFile = Join-Path $WorkingDirectory 'official-desktop.msix'
Invoke-WebRequest -Uri $uri -OutFile $packageFile
try {
    $zip = [IO.Compression.ZipFile]::OpenRead($packageFile)
    try {
        $manifestEntry = $zip.GetEntry('AppxManifest.xml')
        if (!$manifestEntry) { throw 'The official package has no manifest.' }
        $reader = [IO.StreamReader]::new($manifestEntry.Open())
        try { $manifest = [xml]$reader.ReadToEnd() } finally { $reader.Dispose() }
        if ($manifest.Package.Identity.Name -ne 'OpenAI.Codex' -or $manifest.Package.Identity.ProcessorArchitecture -ne 'x64') { throw 'The downloaded package identity does not match the configured channel.' }
        $entry = $zip.GetEntry('app/resources/codex.exe')
        if (!$entry) { throw 'The official package has no bundled backend.' }
        $officialCli = Join-Path $WorkingDirectory 'official-codex.exe'
        $inputFile = $entry.Open()
        try {
            $outputFile = [IO.File]::Create($officialCli)
            try { $inputFile.CopyTo($outputFile) } finally { $outputFile.Dispose() }
        } finally { $inputFile.Dispose() }
    } finally { $zip.Dispose() }
    $versionText = (& $officialCli --version).Trim()
    if ($LASTEXITCODE -ne 0 -or $versionText -notmatch '^codex-cli (\S+)$') { throw 'The official backend did not report its version.' }
    $version = $Matches[1]
    [void][System.Management.Automation.SemanticVersion]::new($version)
    [pscustomobject]@{ package_version=[string]$manifest.Package.Identity.Version; backend_version=$version }
} finally {
    [IO.File]::Delete($packageFile)
    [IO.File]::Delete((Join-Path $WorkingDirectory 'official-codex.exe'))
}
