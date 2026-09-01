[CmdletBinding()]
param(
    [Parameter()]
    [string] $ManifestPath = (Join-Path $PSScriptRoot "../third_party/pdfium/provenance.json")
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-ConfiguredString {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Name,

        [Parameter(Mandatory = $true)]
        [AllowNull()]
        [object] $Value
    )

    if (($Value -isnot [string]) -or [string]::IsNullOrWhiteSpace($Value)) {
        throw "PDFium manifest value '$Name' must be a non-empty string."
    }

    if ($Value.Trim() -match "(?i)^(TODO|TBD|UNSET)(?:\b|_)") {
        throw "PDFium manifest value '$Name' is still a placeholder."
    }
}

function Resolve-ArchiveEntry {
    param(
        [Parameter(Mandatory = $true)]
        [string] $ExtractionRoot,

        [Parameter(Mandatory = $true)]
        [string] $RelativePath
    )

    $normalizedRelativePath = $RelativePath.Replace("/", [IO.Path]::DirectorySeparatorChar)
    if ([IO.Path]::IsPathRooted($normalizedRelativePath)) {
        throw "Archive entry path '$RelativePath' must be relative."
    }

    $root = [IO.Path]::GetFullPath($ExtractionRoot)
    $candidate = [IO.Path]::GetFullPath((Join-Path $root $normalizedRelativePath))
    $rootPrefix = $root.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $candidate.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Archive entry path '$RelativePath' escapes the extraction directory."
    }

    return $candidate
}

if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
    throw "PDFium manifest not found: $ManifestPath"
}

$manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
if ($manifest.schemaVersion -ne 1) {
    throw "Unsupported PDFium manifest schema version '$($manifest.schemaVersion)'."
}
if ($manifest.platform -ne "windows-x64") {
    throw "PDFium manifest platform must be 'windows-x64'."
}
$requiredValues = [ordered]@{
    "archiveFormat" = $manifest.archiveFormat
    "version" = $manifest.version
    "downloadUrl" = $manifest.downloadUrl
    "sha256" = $manifest.sha256
    "attestation.type" = $manifest.attestation.type
    "attestation.url" = $manifest.attestation.url
    "license.spdx" = $manifest.license.spdx
    "license.url" = $manifest.license.url
    "archiveFiles.pdfiumDll" = $manifest.archiveFiles.pdfiumDll
    "archiveFiles.pdfiumImportLibrary" = $manifest.archiveFiles.pdfiumImportLibrary
    "archiveFiles.headersDirectory" = $manifest.archiveFiles.headersDirectory
}
foreach ($entry in $requiredValues.GetEnumerator()) {
    Assert-ConfiguredString -Name $entry.Key -Value $entry.Value
}
if ($manifest.archiveFormat -notin @("zip", "tar.gz")) {
    throw "PDFium manifest archiveFormat must be 'zip' or 'tar.gz'."
}

$downloadUri = $null
if ((-not [Uri]::TryCreate($manifest.downloadUrl, [UriKind]::Absolute, [ref] $downloadUri)) -or
    ($downloadUri.Scheme -ne [Uri]::UriSchemeHttps)) {
    throw "PDFium downloadUrl must be an absolute HTTPS URL."
}
foreach ($urlField in @("attestation.url", "license.url")) {
    $uri = $null
    if ((-not [Uri]::TryCreate($requiredValues[$urlField], [UriKind]::Absolute, [ref] $uri)) -or
        ($uri.Scheme -ne [Uri]::UriSchemeHttps)) {
        throw "PDFium manifest value '$urlField' must be an absolute HTTPS URL."
    }
}
if ($manifest.sha256 -notmatch "^[A-Fa-f0-9]{64}$") {
    throw "PDFium sha256 must contain exactly 64 hexadecimal characters."
}

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$stagePath = Join-Path $repositoryRoot "third_party/pdfium/artifacts/windows-x64"
$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("inkwell-pdfium-" + [Guid]::NewGuid().ToString("N"))
$archiveName = if ($manifest.archiveFormat -eq "zip") { "pdfium.zip" } else { "pdfium.tar.gz" }
$archivePath = Join-Path $workRoot $archiveName
$extractionPath = Join-Path $workRoot "extracted"
$candidateStagePath = Join-Path (Split-Path -Parent $stagePath) (".windows-x64-" + [Guid]::NewGuid().ToString("N"))
$backupStagePath = $null

try {
    New-Item -ItemType Directory -Path $workRoot | Out-Null
    New-Item -ItemType Directory -Path $extractionPath | Out-Null

    Invoke-WebRequest -Uri $downloadUri -OutFile $archivePath -UseBasicParsing
    $actualSha256 = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
    if ($actualSha256 -ine $manifest.sha256) {
        throw "PDFium archive SHA-256 mismatch. Expected $($manifest.sha256), received $actualSha256."
    }

    if ($manifest.archiveFormat -eq "zip") {
        Expand-Archive -LiteralPath $archivePath -DestinationPath $extractionPath
    }
    else {
        $tar = Get-Command "tar.exe" -ErrorAction Stop
        & $tar.Source -xzf $archivePath -C $extractionPath
        if ($LASTEXITCODE -ne 0) {
            throw "PDFium tar.gz extraction failed with exit code $LASTEXITCODE."
        }
    }

    $dllSource = Resolve-ArchiveEntry -ExtractionRoot $extractionPath -RelativePath $manifest.archiveFiles.pdfiumDll
    $importLibrarySource = Resolve-ArchiveEntry -ExtractionRoot $extractionPath -RelativePath $manifest.archiveFiles.pdfiumImportLibrary
    $headersSource = Resolve-ArchiveEntry -ExtractionRoot $extractionPath -RelativePath $manifest.archiveFiles.headersDirectory
    if (-not (Test-Path -LiteralPath $dllSource -PathType Leaf)) {
        throw "Configured PDFium DLL was not found in the archive: $($manifest.archiveFiles.pdfiumDll)"
    }
    if (-not (Test-Path -LiteralPath $importLibrarySource -PathType Leaf)) {
        throw "Configured PDFium import library was not found in the archive: $($manifest.archiveFiles.pdfiumImportLibrary)"
    }
    if (-not (Test-Path -LiteralPath $headersSource -PathType Container)) {
        throw "Configured PDFium headers directory was not found in the archive: $($manifest.archiveFiles.headersDirectory)"
    }

    New-Item -ItemType Directory -Path (Join-Path $candidateStagePath "bin") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $candidateStagePath "lib") -Force | Out-Null
    Copy-Item -LiteralPath $dllSource -Destination (Join-Path $candidateStagePath "bin/pdfium.dll")
    Copy-Item -LiteralPath $importLibrarySource -Destination (Join-Path $candidateStagePath "lib/pdfium.lib")
    Copy-Item -LiteralPath $headersSource -Destination (Join-Path $candidateStagePath "include") -Recurse

    if (Test-Path -LiteralPath $stagePath) {
        $backupStagePath = Join-Path (Split-Path -Parent $stagePath) (".windows-x64-backup-" + [Guid]::NewGuid().ToString("N"))
        Move-Item -LiteralPath $stagePath -Destination $backupStagePath
    }

    try {
        Move-Item -LiteralPath $candidateStagePath -Destination $stagePath
    }
    catch {
        if (($null -ne $backupStagePath) -and (Test-Path -LiteralPath $backupStagePath)) {
            Move-Item -LiteralPath $backupStagePath -Destination $stagePath
            $backupStagePath = $null
        }
        throw
    }

    if (($null -ne $backupStagePath) -and (Test-Path -LiteralPath $backupStagePath)) {
        Remove-Item -LiteralPath $backupStagePath -Recurse -Force
        $backupStagePath = $null
    }

    Write-Output "Prepared PDFium $($manifest.version) for Windows x64 at $stagePath"
}
finally {
    if (Test-Path -LiteralPath $candidateStagePath) {
        Remove-Item -LiteralPath $candidateStagePath -Recurse -Force
    }
    if (Test-Path -LiteralPath $workRoot) {
        Remove-Item -LiteralPath $workRoot -Recurse -Force
    }
}
