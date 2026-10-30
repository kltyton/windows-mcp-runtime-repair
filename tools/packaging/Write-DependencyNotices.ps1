param(
    [Parameter(Mandatory)][string]$ManifestPath,
    [Parameter(Mandatory)][string]$RootPackage,
    [Parameter(Mandatory)][string]$Destination
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot '..\licensing\Read-PublishedLicense.ps1')
$bundleRoot=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\licenses\dependencies'))
$cargo=if($env:RUSTC) {Join-Path (Split-Path $env:RUSTC) 'cargo.exe'} else {'cargo'}
$raw=& $cargo metadata --manifest-path $ManifestPath --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc
if($LASTEXITCODE -ne 0) {throw 'Could not read the locked dependency licenses.'}
$metadata=($raw -join [Environment]::NewLine) | ConvertFrom-Json
$packages=@{}; $nodes=@{}
foreach($package in $metadata.packages) {$packages[$package.id]=$package}
foreach($node in $metadata.resolve.nodes) {$nodes[$node.id]=$node}
$root=@($metadata.packages | Where-Object {$_.name -ceq $RootPackage -and !$_.source})
if($root.Count -ne 1) {throw 'The licensed root package is ambiguous.'}
$queue=[Collections.Generic.Queue[string]]::new()
$seen=[Collections.Generic.HashSet[string]]::new()
$queue.Enqueue($root[0].id)
while($queue.Count) {
    $id=$queue.Dequeue()
    if(!$seen.Add($id)) {continue}
    foreach($dependency in $nodes[$id].deps) {
        if(@($dependency.dep_kinds | Where-Object {$_.kind -ne 'dev'}).Count) {$queue.Enqueue($dependency.pkg)}
    }
}
$text=[Text.StringBuilder]::new()
[void]$text.AppendLine('Third-party dependency licenses and notices')
$missing=[Collections.Generic.List[string]]::new()
foreach($package in @($seen | ForEach-Object {$packages[$_]} | Where-Object source | Sort-Object name,version)) {
    [void]$text.AppendLine("`n=== $($package.name) $($package.version) ===")
    [void]$text.AppendLine("License: $($package.license)")
    $directory=Split-Path -LiteralPath $package.manifest_path
    $files=@(Get-ChildItem -LiteralPath $directory -File | Where-Object Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)([._-].*)?$')
    if($package.license_file) {
        $explicit=if([IO.Path]::IsPathRooted($package.license_file)) {$package.license_file} else {[IO.Path]::GetFullPath((Join-Path $directory $package.license_file))}
        if(@($files | Where-Object FullName -eq $explicit).Count -eq 0) {$files+=Get-Item -LiteralPath $explicit}
    }
    foreach($folder in @('LICENSES','licenses')) {
        $licenseFolder=Join-Path $directory $folder
        if(Test-Path -LiteralPath $licenseFolder -PathType Container) {$files+=Get-ChildItem -LiteralPath $licenseFolder -File -Recurse}
    }
    $published=@()
    if(!$files.Count) {$published=@(Read-PublishedLicense -Package $package -Packages $metadata.packages -BundleRoot $bundleRoot)}
    if(!$files.Count -and !$published.Count) {$missing.Add($package.name+' '+$package.version);continue}
    foreach($file in @($files | Sort-Object FullName -Unique)) {
        $boundary=[IO.DirectoryInfo]::new($directory)
        if($package.source -like 'git+*') {
            $ancestor=$boundary
            while($ancestor) {
                if(Test-Path -LiteralPath (Join-Path $ancestor.FullName '.git')) {$boundary=$ancestor;break}
                $ancestor=$ancestor.Parent
            }
        }
        $prefix=$boundary.FullName.TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar
        if(!$file.FullName.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)) {throw 'A dependency license escaped its public source directory.'}
        if($file.Attributes -band [IO.FileAttributes]::ReparsePoint) {throw 'A local license link requires explicit source resolution.'}
        $ancestor=$file.Directory
        while($ancestor.FullName -ine $boundary.FullName) {
            if($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) {throw 'A local license directory contains a reparse point.'}
            $ancestor=$ancestor.Parent
        }
        [void]$text.AppendLine('--- '+$file.Name+' ---')
        [void]$text.AppendLine([IO.File]::ReadAllText($file.FullName))
    }
    foreach($file in $published) {
        [void]$text.AppendLine('--- '+$file.name+' ---')
        if($file.selectedLicense) {[void]$text.AppendLine('Selected license: '+$file.selectedLicense)}
        [void]$text.AppendLine('Source: '+$file.source)
        [void]$text.AppendLine($file.text)
    }
}
if($missing.Count) {throw ('Dependency license files are missing: '+($missing -join ', '))}
[IO.File]::WriteAllText($Destination,$text.ToString(),[Text.UTF8Encoding]::new($false))
