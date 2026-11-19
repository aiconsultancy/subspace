<#
.SYNOPSIS
Download a Subspace release with the GitHub CLI and install it for the
current user.

.DESCRIPTION
Releases use the subspace-desktop-v tag prefix. The GitHub CLI must be
installed and signed in. Override the repository with -Repository OWNER/REPO
or $env:SUBSPACE_RELEASE_REPO.

The script picks the newest release, or -Version, downloads the archive for
this machine and its .sha256 file, verifies the checksum, and installs.

On macOS the signed app goes to ~/Applications and its executable is linked
into -BinDir or ~/.local/bin. On Windows the executable goes to a
product-specific directory under %LOCALAPPDATA%\Programs and the script adds
that directory to the user PATH; -NoPath skips that change. On Linux the
executable goes to -BinDir or ~/.local/bin.

.EXAMPLE
./install-release.ps1

.EXAMPLE
./install-release.ps1 -Version 0.1.0 -BinDir C:\tools\bin -NoPath
#>
[CmdletBinding()]
param(
    [string]$Repository = $(if ($env:SUBSPACE_RELEASE_REPO) { $env:SUBSPACE_RELEASE_REPO } else { 'aiconsultancy/subspace' }),
    [string]$Version = $(if ($env:SUBSPACE_VERSION) { $env:SUBSPACE_VERSION } else { 'latest' }),
    [string]$BinDir = $env:SUBSPACE_BIN_DIR,
    [ValidateSet('stable', 'nightly', 'alpha', 'beta')]
    [string]$Channel = 'stable',
    [switch]$Link,
    [switch]$NoPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$Channel = $Channel.ToLowerInvariant()
$productSettings = @{
    TagPrefix = 'subspace-desktop-v'
    AssetPrefix = 'subspace'
    Executable = 'subspace'
    WindowsDirectory = 'subspace'
    DisplayName = 'Subspace'
    AppName = 'Subspace'
}
$tagPrefix = $productSettings.TagPrefix
$assetStem = $productSettings.AppName
$shouldLink = $Channel -eq 'stable' -or $Link
if ($Channel -ne 'stable') {
    $title = [char]::ToUpperInvariant($Channel[0]) + $Channel.Substring(1)
    $productSettings.AppName += " ($title)"
    $assetStem += "-$title"
    $productSettings.WindowsDirectory += "-$Channel"
}
$versionPattern = '(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'
if ($Channel -ne 'stable') { $versionPattern += "-$Channel\.[1-9][0-9]{11}\.g[0-9a-f]{7}" }
if ($Version -ne 'latest') {
    $Version = $Version.TrimStart('v')
    if ($Version -cnotmatch "^$versionPattern`$") { throw "version $Version is not a $Channel version" }
}

if ([string]::IsNullOrWhiteSpace($Repository)) {
    throw 'a release repository is required'
}
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw 'required tool not found: gh (the GitHub CLI)'
}

function Get-OS {
    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)) {
        return 'linux'
    }
    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)) {
        return 'darwin'
    }
    return 'windows'
}

function Get-Architecture {
    # PROCESSOR_ARCHITECTURE reports the architecture of the current process,
    # which is wrong under emulation. The runtime value describes the machine.
    $architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    switch ($architecture.ToString()) {
        'X64' { return 'x86_64' }
        'Arm64' { return 'aarch64' }
        default { throw "unsupported architecture: $architecture" }
    }
}

function Get-DefaultBinDir {
    param([string]$OperatingSystem, [string]$WindowsDirectory)

    if ($OperatingSystem -eq 'windows') {
        if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
            throw 'LOCALAPPDATA is empty; pass -BinDir instead'
        }
        return (Join-Path $env:LOCALAPPDATA "Programs\$WindowsDirectory")
    }

    $homeDirectory = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
    if ([string]::IsNullOrWhiteSpace($homeDirectory)) {
        throw 'HOME and USERPROFILE are both empty; pass -BinDir instead'
    }
    return (Join-Path $homeDirectory '.local/bin')
}

# Replace a binary that may be running. Windows refuses to overwrite or delete
# a running executable but allows it to be renamed, so the old file moves
# aside first and is removed when it is no longer in use.
function Install-Binary {
    param([string]$Source, [string]$Target, [string]$OperatingSystem)

    $staged = "$Target.new$PID"
    Copy-Item -LiteralPath $Source -Destination $staged -Force
    if ($OperatingSystem -ne 'windows') { chmod 755 $staged }

    $previous = "$Target.old$PID"
    $hadTarget = Test-Path -LiteralPath $Target
    try {
        if ($OperatingSystem -eq 'windows' -and $hadTarget) {
            Move-Item -LiteralPath $Target -Destination $previous -Force
        }
        Move-Item -LiteralPath $staged -Destination $Target -Force
    } catch {
        Remove-Item -LiteralPath $staged -Force -ErrorAction SilentlyContinue
        if ($hadTarget -and (Test-Path -LiteralPath $previous) -and -not (Test-Path -LiteralPath $Target)) {
            Move-Item -LiteralPath $previous -Destination $Target -Force
        }
        throw
    }
    Remove-Item -LiteralPath $previous -Force -ErrorAction SilentlyContinue
}

if ($Version -eq 'latest') {
    if ($Channel -eq 'stable') {
    $jq = "[.[] | select(.tagName | startswith(`"$tagPrefix`"))][0].tagName // empty"
    $tag = (& gh release list --repo $Repository --exclude-drafts --exclude-pre-releases --limit 100 --json tagName --jq $jq | Out-String).Trim()
    } else {
        # gh applies --jq to each page and rejects --slurp with --jq, so the
        # filter prints matching tags and the newest fixed-width ID wins here.
        # [.] instead of \. keeps the regex free of argument-passing escapes.
        $jq = ".[] | select(.draft == false and .prerelease == true) | .tag_name | select(test(`"^${tagPrefix}[0-9]+[.][0-9]+[.][0-9]+-${Channel}[.][1-9][0-9]{11}[.]g[0-9a-f]{7}`$`"))"
        $tags = @(& gh api --paginate "repos/$Repository/releases?per_page=100" --jq $jq)
        $marker = "-$Channel."
        $tag = $tags | Where-Object { $_ } |
            Sort-Object { $_.Substring($_.IndexOf($marker) + $marker.Length, 12) } |
            Select-Object -Last 1
        $tag = "$tag".Trim()
    }
    if ($LASTEXITCODE -ne 0) { throw "could not list the releases of $Repository" }
    if (-not $tag) {
        throw "no published release found with a $tagPrefix tag in $Repository; use -Version to select an older release"
    }
    $Version = $tag.Substring($tagPrefix.Length)
} else {
    $Version = $Version.TrimStart('v')
    $tag = "$tagPrefix$Version"
}
if ($Version -cnotmatch "^$versionPattern`$") { throw "version $Version is not a $Channel version" }

$goos = Get-OS
$arch = Get-Architecture
switch ($goos) {
    'linux' { $target = "$arch-unknown-linux-gnu" }
    'darwin' { $target = "$arch-apple-darwin" }
    default { $target = "$arch-pc-windows-msvc" }
}
$unpublished = @{
    'x86_64-apple-darwin'     = 'Intel Macs'
    'aarch64-pc-windows-msvc' = 'Windows arm64'
}
if ($unpublished.ContainsKey($target)) {
    throw "no $($productSettings.DisplayName) release is published for $($unpublished[$target]) ($target)"
}
$extension = if ($goos -eq 'windows' -or $goos -eq 'darwin') { 'zip' } else { 'tar.gz' }
$exe = if ($goos -eq 'windows') { '.exe' } else { '' }
$assetName = if ($goos -eq 'darwin') {
    "${assetStem}_${Version}_darwin_arm64.zip"
} else {
    "$($productSettings.AssetPrefix)-$Version-$target.$extension"
}

$workingDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("subspace-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $workingDirectory -Force | Out-Null
try {
    Write-Host "downloading $assetName from $tag"
    & gh release download $tag --repo $Repository --dir $workingDirectory --pattern $assetName --pattern "$assetName.sha256"
    if ($LASTEXITCODE -ne 0) {
        throw "could not download $assetName and $assetName.sha256 from release $tag in $Repository"
    }
    $archivePath = Join-Path $workingDirectory $assetName
    $checksumPath = "$archivePath.sha256"
    foreach ($path in @($archivePath, $checksumPath)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "release $tag has no asset named $(Split-Path -Leaf $path)"
        }
    }

    $expected = $null
    foreach ($line in Get-Content -LiteralPath $checksumPath) {
        $fields = @($line -split '\s+' | Where-Object { $_ })
        if ($fields.Count -ge 2 -and $fields[1].TrimStart('*') -eq $assetName) {
            $expected = $fields[0]
            break
        }
    }
    if (-not $expected) {
        throw "$assetName.sha256 has no entry for $assetName"
    }
    $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
    if ($actual -ne $expected.ToUpperInvariant()) {
        throw "checksum mismatch for ${assetName}: expected $expected, actual $actual"
    }
    Write-Host 'checksum verified'

    $extractDirectory = Join-Path $workingDirectory 'extract'
    New-Item -ItemType Directory -Path $extractDirectory -Force | Out-Null
    if ($goos -eq 'darwin') {
        # ditto keeps the app's executable modes and extended attributes;
        # Expand-Archive does not reliably keep them.
        ditto -x -k $archivePath $extractDirectory
        if ($LASTEXITCODE -ne 0) { throw "failed to extract $assetName" }
    } elseif ($extension -eq 'zip') {
        Expand-Archive -LiteralPath $archivePath -DestinationPath $extractDirectory -Force
    } else {
        tar -xzf $archivePath -C $extractDirectory
        if ($LASTEXITCODE -ne 0) { throw "failed to extract $assetName" }
    }

    if ([string]::IsNullOrWhiteSpace($BinDir)) {
        $BinDir = Get-DefaultBinDir -OperatingSystem $goos -WindowsDirectory $productSettings.WindowsDirectory
    }
    $linkDirectory = $BinDir
    if ($Channel -ne 'stable' -and $goos -ne 'darwin') {
        if ($goos -eq 'windows') {
            $BinDir = Get-DefaultBinDir -OperatingSystem $goos -WindowsDirectory $productSettings.WindowsDirectory
        } else {
            $homeDirectory = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
            if (-not $homeDirectory) { throw 'HOME and USERPROFILE are empty' }
            $BinDir = Join-Path $homeDirectory ".local/share/$($productSettings.Executable)-$Channel/bin"
        }
    }
    if ($goos -ne 'darwin' -or $shouldLink) { New-Item -ItemType Directory -Path $BinDir -Force | Out-Null }

    $appSource = if ($goos -eq 'darwin') { Join-Path $extractDirectory "$($productSettings.AppName).app" } else { $null }
    $source = if ($goos -eq 'darwin') {
        Join-Path $appSource "Contents/MacOS/$($productSettings.Executable)"
    } else {
        Join-Path $extractDirectory "$($productSettings.Executable)$exe"
    }
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "the archive did not contain $($productSettings.Executable)$exe"
    }
    $installed = Join-Path $BinDir "$($productSettings.Executable)$exe"
    if ($goos -eq 'darwin') {
        # Check the CLI link path before the app is replaced, so a failure
        # cannot leave a new app with a missing command.
        $existing = Get-Item -LiteralPath $installed -Force -ErrorAction SilentlyContinue
        if ($shouldLink -and $existing -and $existing.PSIsContainer -and -not $existing.LinkType) {
            throw "$installed is a directory; remove it or choose another -BinDir"
        }
        $homeDirectory = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
        if ([string]::IsNullOrWhiteSpace($homeDirectory)) { throw 'HOME and USERPROFILE are empty' }
        $applications = Join-Path $homeDirectory 'Applications'
        $appInstalled = Join-Path $applications "$($productSettings.AppName).app"
        $appStaged = "$appInstalled.new$PID"
        New-Item -ItemType Directory -Path $applications -Force | Out-Null
        Remove-Item -LiteralPath $appStaged -Recurse -Force -ErrorAction SilentlyContinue
        Move-Item -LiteralPath $appSource -Destination $appStaged
        Remove-Item -LiteralPath $appInstalled -Recurse -Force -ErrorAction SilentlyContinue
        Move-Item -LiteralPath $appStaged -Destination $appInstalled
        Write-Host "installed $($productSettings.AppName).app $Version to $appInstalled"
        $appExecutable = Join-Path $appInstalled "Contents/MacOS/$($productSettings.Executable)"
        if ($shouldLink) {
            Remove-Item -LiteralPath $installed -Force -ErrorAction SilentlyContinue
            New-Item -ItemType SymbolicLink -Path $installed -Target $appExecutable | Out-Null
            Write-Host "linked $($productSettings.Executable) to $installed"
        }
        $installed = $appExecutable
    } else {
        Install-Binary -Source $source -Target $installed -OperatingSystem $goos
        Write-Host "installed $($productSettings.Executable) $Version to $installed"
        if ($Channel -ne 'stable' -and $shouldLink -and $goos -eq 'linux') {
            New-Item -ItemType Directory -Path $linkDirectory -Force | Out-Null
            $cliLink = Join-Path $linkDirectory $productSettings.Executable
            $existing = Get-Item -LiteralPath $cliLink -Force -ErrorAction SilentlyContinue
            if ($existing -and $existing.PSIsContainer -and -not $existing.LinkType) { throw "$cliLink is a directory" }
            Remove-Item -LiteralPath $cliLink -Force -ErrorAction SilentlyContinue
            New-Item -ItemType SymbolicLink -Path $cliLink -Target $installed | Out-Null
            $BinDir = $linkDirectory
        }
    }

    if ($Channel -ne 'stable' -and -not $shouldLink) {
        Write-Host "CLI: $installed"
        return
    }

    $pathSeparator = [System.IO.Path]::PathSeparator
    $trimmedBinDir = $BinDir.TrimEnd('\', '/')
    if ($goos -eq 'windows' -and -not $NoPath) {
        # The user PATH in the registry, not the PATH of this process, which
        # also holds the machine entries.
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $entries = @(if ($userPath) { $userPath -split $pathSeparator | Where-Object { $_ } })
        if ($Channel -ne 'stable') {
            $newPath = (@($trimmedBinDir) + @($entries | Where-Object { $_ -ne $trimmedBinDir })) -join $pathSeparator
            [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
            $env:Path = (@($trimmedBinDir) + @($env:Path -split $pathSeparator | Where-Object { $_ -ne $trimmedBinDir })) -join $pathSeparator
            Write-Host "selected $trimmedBinDir first on the user PATH; new terminals pick it up"
        } elseif ($entries -notcontains $trimmedBinDir) {
            $newPath = (@($entries) + $trimmedBinDir) -join $pathSeparator
            [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
            Write-Host "added $trimmedBinDir to the user PATH; new terminals pick it up"
        }
        if (($env:Path -split $pathSeparator) -notcontains $trimmedBinDir) {
            $env:Path = "$env:Path$pathSeparator$trimmedBinDir"
        }
    } elseif (($env:PATH -split $pathSeparator) -notcontains $trimmedBinDir) {
        Write-Host ''
        Write-Host "$trimmedBinDir is not on PATH. Add it to your shell profile."
    }
} finally {
    Remove-Item -LiteralPath $workingDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
