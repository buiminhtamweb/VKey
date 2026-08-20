[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$DestinationPath
)

$ErrorActionPreference = "Stop"
$downloadUrl = "https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip"
$expectedSha256 = "6ac824e1642d6f7277d0ed7ea09411a508f6116ba6fae0aa5f2c7daa2ff43d31"
$destination = [System.IO.Path]::GetFullPath($DestinationPath)
$parent = Split-Path -Parent $destination
$token = [Guid]::NewGuid().ToString("N")
$archivePath = Join-Path $parent "wix314-$token.zip"
$extractPath = Join-Path $parent "wix314-$token"

$candlePath = Join-Path $destination "candle.exe"
$lightPath = Join-Path $destination "light.exe"
if ((Test-Path -LiteralPath $candlePath) -and (Test-Path -LiteralPath $lightPath)) {
    Write-Host "[done] Using cached WiX Toolset: $destination"
    exit 0
}

New-Item -ItemType Directory -Path $parent -Force | Out-Null

try {
    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $curl = Get-Command "curl.exe" -ErrorAction SilentlyContinue
    if ($null -ne $curl) {
        & $curl.Source --fail --location --retry 3 --connect-timeout 30 `
            --output $archivePath $downloadUrl
        if ($LASTEXITCODE -ne 0) {
            throw "curl.exe failed to download WiX Toolset (exit code $LASTEXITCODE)."
        }
    }
    else {
        Invoke-WebRequest -UseBasicParsing -Uri $downloadUrl -OutFile $archivePath
    }

    $actualSha256 = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualSha256 -ne $expectedSha256) {
        throw "WiX archive checksum mismatch. Expected $expectedSha256, received $actualSha256."
    }

    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractPath -Force
    if (-not (Test-Path -LiteralPath (Join-Path $extractPath "candle.exe")) -or
        -not (Test-Path -LiteralPath (Join-Path $extractPath "light.exe"))) {
        throw "The downloaded WiX archive does not contain candle.exe and light.exe."
    }

    if (Test-Path -LiteralPath $destination) {
        Remove-Item -LiteralPath $destination -Recurse -Force
    }
    Move-Item -LiteralPath $extractPath -Destination $destination
    Write-Host "[done] Portable WiX Toolset: $destination"
}
finally {
    if (Test-Path -LiteralPath $archivePath) {
        Remove-Item -LiteralPath $archivePath -Force
    }
    if (Test-Path -LiteralPath $extractPath) {
        Remove-Item -LiteralPath $extractPath -Recurse -Force
    }
}
