$script:licenseObjects=@{}
$script:publicationRecords=@{}
$script:licenseWebSession=[Microsoft.PowerShell.Commands.WebRequestSession]::new()

function Invoke-LicenseRequest([string]$Url,[hashtable]$Transport) {
    for($attempt=0;$attempt -lt 3;$attempt++) {
        try {return Invoke-RestMethod -Uri $Url -WebSession $script:licenseWebSession @Transport}
        catch {
            $cause=$_.Exception; $interrupted=$false
            while($cause) {
                if($cause -is [IO.IOException]) {$interrupted=$true}
                $cause=$cause.InnerException
            }
            if(!$interrupted -or $attempt -eq 2) {throw}
            $script:licenseWebSession.Dispose()
            $script:licenseWebSession=[Microsoft.PowerShell.Commands.WebRequestSession]::new()
            Write-Host 'Reconnecting the interrupted read-only license transfer.'
        }
    }
}

function Get-PublicationRecord($Package) {
    $path=Join-Path (Split-Path -LiteralPath $Package.manifest_path) '.cargo_vcs_info.json'
    if(!$script:publicationRecords.ContainsKey($path)) {
        $script:publicationRecords[$path]=if(Test-Path -LiteralPath $path -PathType Leaf) {Get-Content -LiteralPath $path -Raw | ConvertFrom-Json} else {$null}
    }
    $script:publicationRecords[$path]
}

function Get-LicenseRepository([string]$Value) {
    if($Value -match '^https://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)(?:/.*)?$') {
        $owner=$Matches[1]; $name=$Matches[2] -replace '\.git$',''
        if($owner -notin @('.','..') -and $name -notin @('.','..')) {$owner+'/'+$name}
    }
}

function Get-LicenseGitObject([string]$Repository,[string]$Kind,[string]$Revision) {
    if($Revision -notmatch '^[0-9a-fA-F]{40}$') {throw 'A license source is not pinned to a Git object.'}
    $url="https://api.github.com/repos/$Repository/git/$Kind/$Revision"
    if(!$script:licenseObjects.ContainsKey($url)) {
        $cacheRoot=if($env:MCP_RUNTIME_LICENSE_CACHE) {$env:MCP_RUNTIME_LICENSE_CACHE} else {Join-Path ([IO.Path]::GetTempPath()) 'runtime-license-git-objects'}
        [void][IO.Directory]::CreateDirectory($cacheRoot)
        $cacheKey=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($url))).ToLowerInvariant()
        $cacheFile=Join-Path $cacheRoot ($cacheKey+'.json')
        if(Test-Path -LiteralPath $cacheFile -PathType Leaf) {
            $script:licenseObjects[$url]=Get-Content -LiteralPath $cacheFile -Raw | ConvertFrom-Json
            return $script:licenseObjects[$url]
        }
        $headers=@{'User-Agent'='windows-mcp-runtime-repair';Accept='application/vnd.github+json';'X-GitHub-Api-Version'='2022-11-28'}
        $token=if($env:GH_TOKEN) {$env:GH_TOKEN} else {$env:GITHUB_TOKEN}
        if($token) {$headers.Authorization='Bearer '+$token}
        $transport=@{Headers=$headers;PreserveAuthorizationOnRedirect=$true;TimeoutSec=60}
        if($env:HTTPS_PROXY) {$transport.Proxy=$env:HTTPS_PROXY}
        $repositoryUrl="https://api.github.com/repos/$Repository"
        try {
            if(!$script:licenseObjects.ContainsKey($repositoryUrl)) {
                $visibility=Invoke-LicenseRequest $repositoryUrl $transport
                if($visibility.private -or $visibility.visibility -cne 'public') {throw 'A dependency license source is not public.'}
                $script:licenseObjects[$repositoryUrl]=$true
            }
            $script:licenseObjects[$url]=Invoke-LicenseRequest $url $transport
            [IO.File]::WriteAllText($cacheFile,($script:licenseObjects[$url] | ConvertTo-Json -Depth 100),[Text.UTF8Encoding]::new($false))
        } catch {
            $cause=$_.Exception
            while($cause.InnerException) {$cause=$cause.InnerException}
            throw "Public license source $url failed: $($cause.Message)"
        }
    }
    $script:licenseObjects[$url]
}

function Read-PublishedLicense($Package,$Packages,[string]$BundleRoot) {
    $record=Get-PublicationRecord $Package
    $revision=if($Package.source -match '^git\+.*#([0-9a-fA-F]{40})$') {$Matches[1]} else {$record.git.sha1}
    $repository=Get-LicenseRepository $Package.repository
    if(!$repository -and $Package.source -match '^git\+(https://github\.com/[^?#]+)[?#]') {
        $repository=Get-LicenseRepository $Matches[1]
    }
    $catalogPath=Join-Path $BundleRoot 'manifest.json'
    if(Test-Path -LiteralPath $catalogPath -PathType Leaf) {
        $catalog=Get-Content -LiteralPath $catalogPath -Raw | ConvertFrom-Json
        $entry=@($catalog.packages | Where-Object {$_.name -ceq $Package.name -and $_.version -ceq $Package.version -and $_.declaredLicense -ceq $Package.license -and !$_.unresolvedReason})
        if($entry.Count -gt 1) {throw 'Conflicting published license clarifications.'}
        if($entry.Count -eq 1) {
            foreach($file in $entry[0].files) {
                $path=[IO.Path]::GetFullPath((Join-Path $BundleRoot $file.path))
                $owner=[IO.Path]::GetFullPath($BundleRoot).TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar
                if(!$path.StartsWith($owner,[StringComparison]::OrdinalIgnoreCase)) {throw 'License clarification escaped its data directory.'}
                if($Package.source -like 'git+*' -and $file.pinnedCommit -cne $revision) {throw 'A Git dependency license clarification has a different source revision.'}
                if((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ine $file.sha256) {throw 'A published license clarification changed.'}
                [pscustomobject]@{name=[IO.Path]::GetFileName($path);text=[IO.File]::ReadAllText($path);source=$file.sourceUrl;selectedLicense=$entry[0].selectedLicense}
            }
            return
        }
    }
    if(!$revision) {return}
    if(!$repository) {
        foreach($sibling in $Packages) {
            if(!$sibling.repository) {continue}
            $siblingRecord=Get-PublicationRecord $sibling
            if($siblingRecord.git.sha1 -ceq $revision) {$repository=Get-LicenseRepository $sibling.repository;if($repository) {break}}
        }
    }
    if(!$repository) {return}
    $tree=Get-LicenseGitObject $repository 'trees' $revision
    if($tree.truncated) {throw 'The non-recursive license tree is incomplete.'}
    $scopes=[Collections.Generic.List[object]]::new()
    $scopes.Add([pscustomobject]@{path='';tree=$tree})
    $directory=''
    foreach($part in @($record.path_in_vcs -split '/' | Where-Object {$_ -ne ''})) {
        if($part -in @('.','..') -or $part.Contains('\')) {throw 'Invalid crate source directory.'}
        $child=@($tree.tree | Where-Object {$_.path -ceq $part -and $_.type -eq 'tree'})
        if($child.Count -ne 1) {throw 'The published crate directory is absent from its source revision.'}
        $tree=Get-LicenseGitObject $repository 'trees' $child[0].sha
        if($tree.truncated) {throw 'The published crate directory listing is incomplete.'}
        $directory=if($directory) {$directory+'/'+$part} else {$part}
        $scopes.Add([pscustomobject]@{path=$directory;tree=$tree})
    }
    $chosen=@()
    for($index=$scopes.Count-1;$index -ge 0;$index--) {
        $scope=$scopes[$index]
        $files=@($scope.tree.tree | Where-Object {$_.type -eq 'blob' -and $_.path -match '^(LICENSE|LICENCE|COPYING|UNLICENSE)([._-].*)?$'})
        foreach($folder in @($scope.tree.tree | Where-Object {$_.type -eq 'tree' -and $_.path -ieq 'licenses'})) {
            $licenses=Get-LicenseGitObject $repository 'trees' $folder.sha
            foreach($file in @($licenses.tree | Where-Object type -eq 'blob')) {
                $files+=[pscustomobject]@{path=$folder.path+'/'+$file.path;sha=$file.sha;mode=$file.mode}
            }
        }
        if($files.Count) {
            foreach($file in $files) {$chosen+=[pscustomobject]@{path=if($scope.path) {$scope.path+'/'+$file.path} else {$file.path};sha=$file.sha;mode=$file.mode}}
            break
        }
    }
    foreach($scope in $scopes) {
        foreach($file in @($scope.tree.tree | Where-Object {$_.type -eq 'blob' -and $_.path -match '^NOTICE([._-].*)?$'})) {
            $chosen+=[pscustomobject]@{path=if($scope.path) {$scope.path+'/'+$file.path} else {$file.path};sha=$file.sha;mode=$file.mode}
        }
    }
    foreach($file in $chosen) {
        if($file.mode -notin @('100644','100755')) {throw 'A source license requires explicit symlink resolution.'}
        $blob=Get-LicenseGitObject $repository 'blobs' $file.sha
        if($blob.encoding -ne 'base64' -or $blob.sha -cne $file.sha) {throw 'A license blob does not match its pinned source.'}
        $bytes=[Convert]::FromBase64String($blob.content)
        if(!$bytes.Length) {throw 'The published license file is empty.'}
        $text=[Text.UTF8Encoding]::new($false,$true).GetString($bytes)
        $escaped=(@($file.path -split '/' | ForEach-Object {[Uri]::EscapeDataString($_)}) -join '/')
        [pscustomobject]@{name=[IO.Path]::GetFileName($file.path);text=$text;source="https://github.com/$repository/blob/$revision/$escaped"}
    }
}
