<#
.SYNOPSIS
Installs a verified jdk-switch release for Windows x64.
.EXAMPLE
.\install.ps1 -InstallDir 'D:\Tools\jsh'
.EXAMPLE
.\install.ps1 -PathAction Add
#>
[CmdletBinding()]
param(
    [string] $InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\jsh'),
    [string] $Version,
    [ValidateSet('Prompt', 'Add', 'Skip')]
    [string] $PathAction = 'Prompt'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not [Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([Runtime.InteropServices.OSPlatform]::Windows)) {
    throw 'This installer runs on Windows only.'
}
if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne [Runtime.InteropServices.Architecture]::X64) {
    throw 'The published Windows binary supports x64 only.'
}
if ([string]::IsNullOrWhiteSpace($InstallDir)) { throw 'Installation directory cannot be empty.' }
if ($InstallDir -match '[;\r\n]') { throw 'Installation directory cannot contain a semicolon or newline.' }
$expandedDir = [Environment]::ExpandEnvironmentVariables($InstallDir)
$installRoot = [IO.Path]::GetFullPath($ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($expandedDir))

function Test-PathContainsDirectory([string] $PathValue, [string] $Directory) {
    foreach ($entry in ($PathValue -split ';')) {
        $entry = [Environment]::ExpandEnvironmentVariables($entry.Trim().Trim('"'))
        if (-not $entry) { continue }
        try {
            $resolved = [IO.Path]::GetFullPath($entry).TrimEnd([char[]] @('\', '/'))
            $target = $Directory.TrimEnd([char[]] @('\', '/'))
            if ([string]::Equals($resolved, $target, [StringComparison]::OrdinalIgnoreCase)) { return $true }
        } catch { continue }
    }
    return $false
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
$isPersistent = (Test-PathContainsDirectory $userPath $installRoot) -or
    (Test-PathContainsDirectory $machinePath $installRoot)
if (-not $isPersistent -and $PathAction -eq 'Prompt') {
    if ([Console]::IsInputRedirected) {
        throw "Directory is not on PATH. Re-run with -PathAction Add or Skip: $installRoot"
    }
    $answer = Read-Host "Add $installRoot to your user PATH? [Y/n]"
    switch -Regex ($answer) {
        '^(|y|yes)$' { $PathAction = 'Add'; break }
        '^(n|no)$' { $PathAction = 'Skip'; break }
        default { throw 'Please answer yes or no.' }
    }
}

$repo = 'nanocm/jdk-switch'
if (-not $Version) {
    $response = Invoke-WebRequest -Uri "https://github.com/$repo/releases/latest" -Method Head -UseBasicParsing
    $finalUri = if ($response.BaseResponse.PSObject.Properties.Name -contains 'RequestMessage') {
        $response.BaseResponse.RequestMessage.RequestUri
    } else {
        $response.BaseResponse.ResponseUri
    }
    if ($finalUri.Host -ne 'github.com' -or
        $finalUri.AbsolutePath -cnotmatch '^/nanocm/(j-switch|jdk-switch)/releases/tag/(v[^/]+)$') {
        throw "Could not identify the latest release: $finalUri"
    }
    $repo = "nanocm/$($Matches[1])"
    $Version = $Matches[2]
}
if ($Version -cnotmatch '^v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$') {
    throw "Invalid release tag: $Version"
}

$asset = "jsh-$Version-windows-x64.zip"
$baseUrl = "https://github.com/$repo/releases/download/$Version"
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([char[]] @('\', '/'))
$tempRoot = Join-Path $tempBase ('jsh-install-' + [guid]::NewGuid().ToString('N'))
$staged = $null
try {
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    $archive = Join-Path $tempRoot $asset
    $checksums = Join-Path $tempRoot 'SHA256SUMS.txt'
    Write-Host "Downloading $asset..."
    Invoke-WebRequest -Uri "$baseUrl/$asset" -OutFile $archive -UseBasicParsing
    Invoke-WebRequest -Uri "$baseUrl/SHA256SUMS.txt" -OutFile $checksums -UseBasicParsing

    $checksumPattern = '^([A-Fa-f0-9]{64})\s+\*?' + [regex]::Escape($asset) + '$'
    $matchingLines = @(Get-Content -LiteralPath $checksums | Where-Object { $_ -match $checksumPattern })
    if ($matchingLines.Count -ne 1) { throw "Missing or ambiguous checksum for $asset" }
    $expected = [regex]::Match($matchingLines[0], $checksumPattern).Groups[1].Value
    $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    if (-not [string]::Equals($actual, $expected, [StringComparison]::OrdinalIgnoreCase)) {
        throw "SHA-256 mismatch for $asset"
    }

    $extractDir = Join-Path $tempRoot 'extracted'
    Expand-Archive -LiteralPath $archive -DestinationPath $extractDir
    $binary = Join-Path $extractDir 'jsh.exe'
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Release archive does not contain jsh.exe' }
    New-Item -ItemType Directory -Path $installRoot -Force | Out-Null
    $target = Join-Path $installRoot 'jsh.exe'
    $staged = Join-Path $installRoot ('.jsh-install-' + [guid]::NewGuid().ToString('N') + '.exe')
    Copy-Item -LiteralPath $binary -Destination $staged
    if (Test-Path -LiteralPath $target -PathType Container) { throw "Expected a file at $target" }
    Move-Item -LiteralPath $staged -Destination $target -Force
    $staged = $null

    if (-not $isPersistent -and $PathAction -eq 'Add') {
        $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
        try {
            $rawPath = [string] $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            $kind = if ($key.GetValueNames() -contains 'Path') {
                $key.GetValueKind('Path')
            } else {
                [Microsoft.Win32.RegistryValueKind]::ExpandString
            }
            $newPath = if ($rawPath.TrimEnd(';')) { $rawPath.TrimEnd(';') + ';' + $installRoot } else { $installRoot }
            $key.SetValue('Path', $newPath, $kind)
        } finally {
            $key.Close()
        }
        try {
            if (-not ('JshEnvironmentChange' -as [type])) {
                Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class JshEnvironmentChange {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wParam,
        string lParam, uint flags, uint timeout, out IntPtr result);
}
'@
            }
            $broadcastResult = [IntPtr]::Zero
            [void] [JshEnvironmentChange]::SendMessageTimeout(
                [IntPtr] 0xffff, 0x1A, [IntPtr]::Zero, 'Environment', 0x2, 5000, [ref] $broadcastResult)
        } catch {
            Write-Warning 'PATH was saved, but Windows could not notify other applications. Sign out if new terminals do not see it.'
        }
        Write-Host "Added $installRoot to your user PATH. Reopen other terminals to use it there."
    } elseif (-not $isPersistent) {
        Write-Warning "PATH was not changed. Add $installRoot to PATH to run jsh by name."
    }

    if (($isPersistent -or $PathAction -eq 'Add') -and
        -not (Test-PathContainsDirectory $env:Path $installRoot)) { $env:Path += ";$installRoot" }
    Write-Host "Installed $Version to $target"
    & $target --version
} finally {
    if ($staged -and (Test-Path -LiteralPath $staged)) { Remove-Item -LiteralPath $staged -Force }
    $verifiedRoot = [IO.Path]::GetFullPath($tempRoot)
    if ($verifiedRoot.StartsWith($tempBase + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -and
        (Test-Path -LiteralPath $verifiedRoot)) {
        Remove-Item -LiteralPath $verifiedRoot -Recurse -Force
    }
}
