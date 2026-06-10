<#
.SYNOPSIS
Publishes one Skill Hub skill version to a full MinIO-backed marketplace.

.DESCRIPTION
This script packages a local skill directory, calculates SHA-256, uploads the
version files, updates the per-skill manifest, reads categories from an external
categories.v1.json file, rebuilds category/search indexes, and uploads
catalog.v1.json last.

Requires MinIO Client:
  https://min.io/docs/minio/windows/reference/minio-mc.html

Before running, either configure an alias:
  mc alias set skillhub http://127.0.0.1:9000 minioadmin minioadmin

Or pass -Endpoint, -AccessKey, and -SecretKey to this script.

.EXAMPLE
.\publish-skill.ps1 `
  -SkillDir .\examples\frontend-reviewer `
  -Namespace official `
  -Alias skillhub `
  -Bucket skill-market

.EXAMPLE
.\publish-skill.ps1 `
  -SkillDir .\examples\frontend-reviewer `
  -Namespace official `
  -Endpoint http://127.0.0.1:9000 `
  -AccessKey minioadmin `
  -SecretKey minioadmin `
  -CreateBucket

.EXAMPLE
.\publish-skill.ps1 `
  -SkillDir .\examples\frontend-reviewer `
  -Namespace official `
  -Alias skillhub `
  -Bucket skill-market `
  -CategoriesPath .\categories.v1.json
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$SkillDir,

    [string]$Namespace,

    [string]$Version,

    [string]$Alias = "skillhub",

    [string]$Bucket = "skill-market",

    [string]$Endpoint,

    [string]$AccessKey,

    [string]$SecretKey,

    [string]$Region,

    [string]$ChangelogPath,

    [string]$SignaturePath,

    # Categories source JSON. Defaults to categories.v1.json next to this script.
    [string]$CategoriesPath,

    [switch]$CreateBucket,

    [switch]$AllowOverwrite,

    [switch]$DryRun,

    [switch]$KeepTemp
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$PackageFileName = "package.zip"
$HashFileName = "package.sha256"
$VersionSkillFileName = "skill.json"
$SignatureFileName = "signature.minisig"
$ChangelogFileName = "changelog.md"
$DefaultCategoriesFileName = "categories.v1.json"

function Write-Step {
    param([string]$Message)
    Write-Host "[skillhub] $Message"
}

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding -ArgumentList @($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Read-Utf8Text {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $encoding = New-Object System.Text.UTF8Encoding -ArgumentList @($false, $true)
    return [System.IO.File]::ReadAllText($Path, $encoding)
}

function Read-JsonFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $raw = Read-Utf8Text -Path $Path
    if ([string]::IsNullOrWhiteSpace($raw)) {
        throw "JSON file is empty: $Path"
    }

    return ($raw | ConvertFrom-Json)
}

function Save-Json {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [object]$Object
    )

    $json = $Object | ConvertTo-Json -Depth 64
    Write-Utf8NoBom -Path $Path -Text ($json + [Environment]::NewLine)
}

function Normalize-Array {
    param([object]$Value)

    if ($null -eq $Value) {
        return @()
    }

    if ($Value -is [string]) {
        if ([string]::IsNullOrWhiteSpace($Value)) {
            return @()
        }

        return @($Value)
    }

    if ($Value -is [System.Array]) {
        return @($Value)
    }

    return @($Value)
}

function Set-JsonProperty {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Object,

        [Parameter(Mandatory = $true)]
        [string]$Name,

        [object]$Value
    )

    if ($Object.PSObject.Properties.Name -contains $Name) {
        $Object.$Name = $Value
    }
    else {
        $Object | Add-Member -NotePropertyName $Name -NotePropertyValue $Value
    }
}

function Join-ObjectPath {
    param(
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$Parts
    )

    return (($Parts | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | ForEach-Object { $_.Trim("/") }) -join "/")
}

function Assert-SafeObjectSegment {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    if ($Value -notmatch "^[A-Za-z0-9][A-Za-z0-9._-]*$") {
        throw "$Name '$Value' is not a safe object path segment. Use letters, numbers, dot, underscore, or dash."
    }
}

function Copy-PackagePayload {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourceDir,

        [Parameter(Mandatory = $true)]
        [string]$DestinationDir,

        [string]$SignatureFileName
    )

    New-Item -ItemType Directory -Path $DestinationDir -Force | Out-Null
    $sourceRoot = (Resolve-Path -LiteralPath $SourceDir).Path

    Get-ChildItem -LiteralPath $sourceRoot -Recurse -Force | Where-Object {
        -not $_.PSIsContainer -and
        $_.Extension -ne ".json" -and
        $_.Name -ne ".DS_Store" -and
        $_.Name -ne $SignatureFileName
    } | ForEach-Object {
        $relative = $_.FullName.Substring($sourceRoot.Length).TrimStart("\", "/")
        $target = Join-Path $DestinationDir $relative
        $parent = Split-Path -Parent $target
        if (-not (Test-Path -LiteralPath $parent)) {
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
        }
        Copy-Item -LiteralPath $_.FullName -Destination $target -Force
    }
}

function Invoke-Mc {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$McArgs
    )

    if ($DryRun) {
        Write-Host "DRY RUN: mc $($McArgs -join ' ')"
        return
    }

    & mc @McArgs
    if ($LASTEXITCODE -ne 0) {
        throw "mc failed: mc $($McArgs -join ' ')"
    }
}

function Test-RemoteObject {
    param([Parameter(Mandatory = $true)][string]$ObjectPath)

    if ($DryRun) {
        return $false
    }

    & mc stat "$Alias/$Bucket/$ObjectPath" | Out-Null
    return ($LASTEXITCODE -eq 0)
}

function Get-RemoteJson {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ObjectPath,

        [Parameter(Mandatory = $true)]
        [object]$DefaultObject,

        [Parameter(Mandatory = $true)]
        [string]$WorkDir
    )

    $localName = (($ObjectPath -replace "[\\/]", "_") -replace "[^A-Za-z0-9._-]", "_")
    $localPath = Join-Path $WorkDir $localName

    if (-not $DryRun) {
        & mc cp "$Alias/$Bucket/$ObjectPath" $localPath | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $localPath)) {
            $raw = Read-Utf8Text -Path $localPath
            if (-not [string]::IsNullOrWhiteSpace($raw)) {
                return ($raw | ConvertFrom-Json)
            }
        }
    }

    return (($DefaultObject | ConvertTo-Json -Depth 64) | ConvertFrom-Json)
}

function Copy-JsonObject {
    param([Parameter(Mandatory = $true)][object]$Object)
    return (($Object | ConvertTo-Json -Depth 64) | ConvertFrom-Json)
}

function Find-DefaultChangelog {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SkillDir
    )

    $candidates = @(
        (Join-Path $SkillDir "changelog.md"),
        (Join-Path $SkillDir "CHANGELOG.md")
    )

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    return $null
}

function Find-DefaultSignature {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SkillDir
    )

    $candidate = Join-Path $SkillDir $SignatureFileName
    if (Test-Path -LiteralPath $candidate) {
        return $candidate
    }

    return $null
}

function New-CatalogDefault {
    return [ordered]@{
        schema = "skillhub.catalog.v1"
        generated_at = $script:Now
        categories = @()
        skills = @()
    }
}

function New-ManifestDefault {
    param(
        [string]$Namespace,
        [string]$SkillId,
        [string]$Name,
        [string]$Summary,
        [object[]]$Categories,
        [object[]]$Tags,
        [object[]]$Levels
    )

    return [ordered]@{
        schema = "skillhub.skill-manifest.v1"
        namespace = $Namespace
        id = $SkillId
        name = $Name
        summary = $Summary
        categories = @($Categories)
        tags = @($Tags)
        levels = @($Levels)
        latest_version = $script:Version
        versions = @()
        updated_at = $script:Now
    }
}

function New-SkillEntry {
    param(
        [string]$Namespace,
        [string]$SkillId,
        [string]$Name,
        [string]$Summary,
        [string]$Version,
        [object[]]$Categories,
        [object[]]$Tags,
        [object[]]$Levels,
        [string]$ManifestPath
    )

    return [ordered]@{
        namespace = $Namespace
        id = $SkillId
        name = $Name
        summary = $Summary
        latest_version = $Version
        categories = @($Categories)
        tags = @($Tags)
        levels = @($Levels)
        manifest_path = $ManifestPath
        updated_at = $script:Now
    }
}

function Build-CategoryIndexes {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Catalog,

        [Parameter(Mandatory = $true)]
        [object]$CategoriesDoc,

        [Parameter(Mandatory = $true)]
        [string]$WorkDir
    )

    $uploaded = @()
    $skills = Normalize-Array $Catalog.skills
    $categoryIds = Normalize-Array $CategoriesDoc.items | ForEach-Object { $_.id } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique

    foreach ($categoryId in $categoryIds) {
        Assert-SafeObjectSegment -Name "category id" -Value $categoryId

        $items = @($skills | Where-Object { (Normalize-Array $_.categories) -contains $categoryId })
        $doc = [ordered]@{
            schema = "skillhub.index.category.v1"
            generated_at = $script:Now
            category = $categoryId
            skills = @($items)
        }

        $path = Join-Path $WorkDir ("index-category-" + $categoryId + ".json")
        Save-Json -Path $path -Object $doc

        $objectPath = Join-ObjectPath "indexes" "category" ($categoryId + ".json")
        Invoke-Mc -McArgs @("cp", $path, "$Alias/$Bucket/$objectPath")
        $uploaded += $objectPath
    }

    return $uploaded
}

function Build-SearchLiteIndex {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Catalog,

        [Parameter(Mandatory = $true)]
        [string]$WorkDir
    )

    $skills = Normalize-Array $Catalog.skills
    $items = @($skills | ForEach-Object {
        [ordered]@{
            key = "$($_.namespace)/$($_.id)"
            namespace = $_.namespace
            id = $_.id
            name = $_.name
            summary = $_.summary
            latest_version = $_.latest_version
            categories = @(Normalize-Array $_.categories)
            tags = @(Normalize-Array $_.tags)
            manifest_path = $_.manifest_path
        }
    })

    $doc = [ordered]@{
        schema = "skillhub.search-lite.v1"
        generated_at = $script:Now
        skills = @($items)
    }

    $path = Join-Path $WorkDir "search-lite.json"
    Save-Json -Path $path -Object $doc

    $objectPath = Join-ObjectPath "indexes" "search-lite.json"
    Invoke-Mc -McArgs @("cp", $path, "$Alias/$Bucket/$objectPath")
    return $objectPath
}

function Assert-SkillCategoriesDefined {
    param(
        [Parameter(Mandatory = $true)]
        [object]$CategoriesDoc,

        [Parameter(Mandatory = $true)]
        [object[]]$SkillCategories,

        [Parameter(Mandatory = $true)]
        [string]$CategoriesPath
    )

    $items = @(Normalize-Array $CategoriesDoc.items)
    $definedIds = @($items | ForEach-Object { $_.id } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })

    foreach ($categoryId in $SkillCategories) {
        Assert-SafeObjectSegment -Name "category id" -Value $categoryId
        $exists = @($definedIds | Where-Object { $_ -eq $categoryId }).Count -gt 0
        if (-not $exists) {
            throw "Category '$categoryId' is used by skill.json but is not defined in $CategoriesPath."
        }
    }

    Set-JsonProperty -Object $CategoriesDoc -Name "schema" -Value "skillhub.categories.v1"
    Set-JsonProperty -Object $CategoriesDoc -Name "generated_at" -Value $script:Now
}

function Read-CategoriesDoc {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Categories file not found: $Path"
    }

    $doc = Read-JsonFile -Path $Path
    if ($doc.schema -ne "skillhub.categories.v1") {
        throw "Categories file schema must be skillhub.categories.v1: $Path"
    }

    if ($null -eq $doc.items) {
        throw "Categories file must contain an items array: $Path"
    }

    $seenIds = @{}
    foreach ($item in @(Normalize-Array $doc.items)) {
        if ([string]::IsNullOrWhiteSpace($item.id)) {
            throw "Category item in $Path must contain id."
        }
        Assert-SafeObjectSegment -Name "category id" -Value $item.id
        if ($seenIds.ContainsKey($item.id)) {
            throw "Duplicate category id '$($item.id)' in $Path."
        }
        $seenIds[$item.id] = $true

        if ([string]::IsNullOrWhiteSpace($item.name)) {
            throw "Category '$($item.id)' in $Path must contain name."
        }
    }

    return $doc
}

function Resolve-FullPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    return [System.IO.Path]::GetFullPath((Resolve-Path -LiteralPath $Path).Path)
}

$script:Now = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
$workDir = Join-Path ([System.IO.Path]::GetTempPath()) ("skillhub-publish-" + [Guid]::NewGuid().ToString("N"))

try {
    if (-not (Get-Command mc -ErrorAction SilentlyContinue)) {
        throw "MinIO Client 'mc' was not found in PATH."
    }

    $skillDirFull = Resolve-FullPath -Path $SkillDir
    if ([string]::IsNullOrWhiteSpace($CategoriesPath)) {
        $CategoriesPath = Join-Path $PSScriptRoot $DefaultCategoriesFileName
    }
    if (-not (Test-Path -LiteralPath $CategoriesPath)) {
        throw "Categories JSON file not found: $CategoriesPath"
    }
    $CategoriesPath = Resolve-FullPath -Path $CategoriesPath

    $sourceSkillJsonPath = Join-Path $skillDirFull $VersionSkillFileName

    if (-not (Test-Path -LiteralPath $sourceSkillJsonPath)) {
        throw "Missing required file: $sourceSkillJsonPath"
    }

    New-Item -ItemType Directory -Path $workDir -Force | Out-Null

    $sourceSkillJson = Read-JsonFile -Path $sourceSkillJsonPath

    if ([string]::IsNullOrWhiteSpace($Namespace)) {
        $Namespace = $sourceSkillJson.namespace
    }

    if ([string]::IsNullOrWhiteSpace($Version)) {
        $Version = $sourceSkillJson.version
    }

    $script:Version = $Version

    $skillId = $sourceSkillJson.id
    $skillName = $sourceSkillJson.name
    $summary = $sourceSkillJson.summary
    $categories = @(Normalize-Array $sourceSkillJson.categories)
    $tags = @(Normalize-Array $sourceSkillJson.tags)
    $levels = @(Normalize-Array $sourceSkillJson.levels)

    if ([string]::IsNullOrWhiteSpace($Namespace)) {
        throw "Namespace is required. Pass -Namespace or set namespace in skill.json."
    }

    if ([string]::IsNullOrWhiteSpace($skillId)) {
        throw "skill.json must contain id."
    }

    if ([string]::IsNullOrWhiteSpace($Version)) {
        throw "Version is required. Pass -Version or set version in skill.json."
    }

    if ([string]::IsNullOrWhiteSpace($skillName)) {
        $skillName = $skillId
    }

    if ($null -eq $summary) {
        $summary = ""
    }

    if ($categories.Count -eq 0) {
        $categories = @("public")
    }

    if ($levels.Count -eq 0) {
        $levels = @("personal", "project")
    }

    Assert-SafeObjectSegment -Name "namespace" -Value $Namespace
    Assert-SafeObjectSegment -Name "skill id" -Value $skillId
    Assert-SafeObjectSegment -Name "version" -Value $Version

    foreach ($category in $categories) {
        Assert-SafeObjectSegment -Name "category id" -Value $category
    }

    if (-not [string]::IsNullOrWhiteSpace($Endpoint)) {
        if ([string]::IsNullOrWhiteSpace($AccessKey) -or [string]::IsNullOrWhiteSpace($SecretKey)) {
            throw "When -Endpoint is provided, -AccessKey and -SecretKey are required."
        }

        $aliasArgs = @("alias", "set", $Alias, $Endpoint, $AccessKey, $SecretKey, "--api", "S3v4")
        if (-not [string]::IsNullOrWhiteSpace($Region)) {
            $aliasArgs += @("--region", $Region)
        }

        Write-Step "Configuring mc alias '$Alias'."
        Invoke-Mc -McArgs $aliasArgs
    }

    if ($CreateBucket) {
        Write-Step "Ensuring bucket '$Bucket' exists."
        Invoke-Mc -McArgs @("mb", "--ignore-existing", "$Alias/$Bucket")
    }

    $skillBaseObject = Join-ObjectPath "skills" $Namespace $skillId
    $versionBaseObject = Join-ObjectPath $skillBaseObject "versions" $Version
    $manifestObject = Join-ObjectPath $skillBaseObject "manifest.json"
    $versionSkillObject = Join-ObjectPath $versionBaseObject $VersionSkillFileName
    $packageObject = Join-ObjectPath $versionBaseObject $PackageFileName
    $hashObject = Join-ObjectPath $versionBaseObject $HashFileName
    $changelogObject = Join-ObjectPath $versionBaseObject $ChangelogFileName
    $signatureObject = Join-ObjectPath $versionBaseObject $SignatureFileName

    if ((Test-RemoteObject -ObjectPath $packageObject) -and -not $AllowOverwrite) {
        throw "Remote package already exists: $packageObject. Use -AllowOverwrite to replace it."
    }

    Write-Step "Packaging skill '$Namespace/$skillId@$Version'."
    $packagePath = Join-Path $workDir $PackageFileName
    $payloadDir = Join-Path $workDir "payload"
    Copy-PackagePayload -SourceDir $skillDirFull -DestinationDir $payloadDir -SignatureFileName $SignatureFileName
    $sourceItems = @(Get-ChildItem -LiteralPath $payloadDir -Force)

    if ($sourceItems.Count -eq 0) {
        throw "Skill package payload is empty after filtering json files: $skillDirFull"
    }

    Compress-Archive -Path @($sourceItems | ForEach-Object { $_.FullName }) -DestinationPath $packagePath -Force

    $packageHash = (Get-FileHash -LiteralPath $packagePath -Algorithm SHA256).Hash.ToLowerInvariant()
    $packageSize = (Get-Item -LiteralPath $packagePath).Length
    $hashPath = Join-Path $workDir $HashFileName
    Write-Utf8NoBom -Path $hashPath -Text ($packageHash + [Environment]::NewLine)

    $generatedSkillJson = Copy-JsonObject -Object $sourceSkillJson
    Set-JsonProperty -Object $generatedSkillJson -Name "schema" -Value "skillhub.skill.v1"
    Set-JsonProperty -Object $generatedSkillJson -Name "namespace" -Value $Namespace
    Set-JsonProperty -Object $generatedSkillJson -Name "id" -Value $skillId
    Set-JsonProperty -Object $generatedSkillJson -Name "name" -Value $skillName
    Set-JsonProperty -Object $generatedSkillJson -Name "version" -Value $Version
    Set-JsonProperty -Object $generatedSkillJson -Name "summary" -Value $summary
    Set-JsonProperty -Object $generatedSkillJson -Name "categories" -Value @($categories)
    Set-JsonProperty -Object $generatedSkillJson -Name "tags" -Value @($tags)
    Set-JsonProperty -Object $generatedSkillJson -Name "levels" -Value @($levels)
    Set-JsonProperty -Object $generatedSkillJson -Name "package" -Value ([ordered]@{
        file = $PackageFileName
        path = $packageObject
        sha256 = $packageHash
        sha256_path = $hashObject
        size = $packageSize
    })

    $generatedSkillJsonPath = Join-Path $workDir $VersionSkillFileName
    Save-Json -Path $generatedSkillJsonPath -Object $generatedSkillJson

    if ([string]::IsNullOrWhiteSpace($ChangelogPath)) {
        $ChangelogPath = Find-DefaultChangelog -SkillDir $skillDirFull
    }

    $generatedChangelogPath = $null
    if ([string]::IsNullOrWhiteSpace($ChangelogPath)) {
        $generatedChangelogPath = Join-Path $workDir $ChangelogFileName
        Write-Utf8NoBom -Path $generatedChangelogPath -Text ("# $Version" + [Environment]::NewLine + [Environment]::NewLine + "- Published by Skill Hub publisher." + [Environment]::NewLine)
        $ChangelogPath = $generatedChangelogPath
    }
    else {
        $ChangelogPath = Resolve-FullPath -Path $ChangelogPath
    }

    if ([string]::IsNullOrWhiteSpace($SignaturePath)) {
        $SignaturePath = Find-DefaultSignature -SkillDir $skillDirFull
    }
    elseif (-not (Test-Path -LiteralPath $SignaturePath)) {
        throw "Signature file not found: $SignaturePath"
    }

    if (-not [string]::IsNullOrWhiteSpace($SignaturePath)) {
        $SignaturePath = Resolve-FullPath -Path $SignaturePath
    }

    Write-Step "Loading remote manifest, catalog, and categories from '$CategoriesPath'."
    $manifest = Get-RemoteJson `
        -ObjectPath $manifestObject `
        -DefaultObject (New-ManifestDefault -Namespace $Namespace -SkillId $skillId -Name $skillName -Summary $summary -Categories $categories -Tags $tags -Levels $levels) `
        -WorkDir $workDir

    $existingVersions = @(Normalize-Array $manifest.versions)
    $existingVersionCount = @($existingVersions | Where-Object { $_.version -eq $Version }).Count
    if ($existingVersionCount -gt 0 -and -not $AllowOverwrite) {
        throw "Version already exists in manifest: $Namespace/$skillId@$Version. Use -AllowOverwrite to replace it."
    }

    $versionEntry = [ordered]@{
        version = $Version
        skill_path = $versionSkillObject
        package_path = $packageObject
        sha256_path = $hashObject
        changelog_path = $changelogObject
        created_at = $script:Now
        package = [ordered]@{
            file = $PackageFileName
            sha256 = $packageHash
            size = $packageSize
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($SignaturePath)) {
        $versionEntry["signature_path"] = $signatureObject
    }

    $updatedVersions = @($existingVersions | Where-Object { $_.version -ne $Version })
    $updatedVersions += $versionEntry

    Set-JsonProperty -Object $manifest -Name "schema" -Value "skillhub.skill-manifest.v1"
    Set-JsonProperty -Object $manifest -Name "namespace" -Value $Namespace
    Set-JsonProperty -Object $manifest -Name "id" -Value $skillId
    Set-JsonProperty -Object $manifest -Name "name" -Value $skillName
    Set-JsonProperty -Object $manifest -Name "summary" -Value $summary
    Set-JsonProperty -Object $manifest -Name "categories" -Value @($categories)
    Set-JsonProperty -Object $manifest -Name "tags" -Value @($tags)
    Set-JsonProperty -Object $manifest -Name "levels" -Value @($levels)
    Set-JsonProperty -Object $manifest -Name "latest_version" -Value $Version
    Set-JsonProperty -Object $manifest -Name "versions" -Value @($updatedVersions)
    Set-JsonProperty -Object $manifest -Name "updated_at" -Value $script:Now

    $manifestPath = Join-Path $workDir "manifest.json"
    Save-Json -Path $manifestPath -Object $manifest

    $categoriesDoc = Read-CategoriesDoc -Path $CategoriesPath
    Assert-SkillCategoriesDefined -CategoriesDoc $categoriesDoc -SkillCategories $categories -CategoriesPath $CategoriesPath

    $catalog = Get-RemoteJson `
        -ObjectPath "catalog.v1.json" `
        -DefaultObject (New-CatalogDefault) `
        -WorkDir $workDir

    $skillEntry = New-SkillEntry `
        -Namespace $Namespace `
        -SkillId $skillId `
        -Name $skillName `
        -Summary $summary `
        -Version $Version `
        -Categories $categories `
        -Tags $tags `
        -Levels $levels `
        -ManifestPath $manifestObject

    $catalogSkills = @(Normalize-Array $catalog.skills | Where-Object { -not ($_.namespace -eq $Namespace -and $_.id -eq $skillId) })
    $catalogSkills += $skillEntry

    $categoryIds = @(Normalize-Array $categoriesDoc.items | ForEach-Object { $_.id } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique)

    Set-JsonProperty -Object $catalog -Name "schema" -Value "skillhub.catalog.v1"
    Set-JsonProperty -Object $catalog -Name "generated_at" -Value $script:Now
    Set-JsonProperty -Object $catalog -Name "categories" -Value @($categoryIds)
    Set-JsonProperty -Object $catalog -Name "skills" -Value @($catalogSkills)

    $generatedCategoriesPath = Join-Path $workDir "categories.v1.json"
    $catalogPath = Join-Path $workDir "catalog.v1.json"
    Save-Json -Path $generatedCategoriesPath -Object $categoriesDoc
    Save-Json -Path $catalogPath -Object $catalog

    Write-Step "Uploading version files."
    Invoke-Mc -McArgs @("cp", $generatedSkillJsonPath, "$Alias/$Bucket/$versionSkillObject")
    Invoke-Mc -McArgs @("cp", $packagePath, "$Alias/$Bucket/$packageObject")
    Invoke-Mc -McArgs @("cp", $hashPath, "$Alias/$Bucket/$hashObject")
    Invoke-Mc -McArgs @("cp", $ChangelogPath, "$Alias/$Bucket/$changelogObject")

    if (-not [string]::IsNullOrWhiteSpace($SignaturePath)) {
        Invoke-Mc -McArgs @("cp", $SignaturePath, "$Alias/$Bucket/$signatureObject")
    }

    Write-Step "Uploading skill manifest."
    Invoke-Mc -McArgs @("cp", $manifestPath, "$Alias/$Bucket/$manifestObject")

    Write-Step "Uploading categories and indexes."
    Invoke-Mc -McArgs @("cp", $generatedCategoriesPath, "$Alias/$Bucket/categories.v1.json")
    $categoryIndexObjects = Build-CategoryIndexes -Catalog $catalog -CategoriesDoc $categoriesDoc -WorkDir $workDir
    $searchIndexObject = Build-SearchLiteIndex -Catalog $catalog -WorkDir $workDir
    Invoke-Mc -McArgs @("rm", "--recursive", "--force", "$Alias/$Bucket/indexes/target")

    Write-Step "Uploading catalog last."
    Invoke-Mc -McArgs @("cp", $catalogPath, "$Alias/$Bucket/catalog.v1.json")

    Write-Host ""
    Write-Host "Published: $Namespace/$skillId@$Version"
    Write-Host "Package:   $packageObject"
    Write-Host "SHA-256:   $packageHash"
    Write-Host "Manifest:  $manifestObject"
    Write-Host "Catalog:   catalog.v1.json"
    Write-Host "Indexes:   $($categoryIndexObjects.Count) category, search-lite"
    if ($DryRun) {
        Write-Host "Mode:      dry run; no objects were uploaded"
    }
}
finally {
    if ($KeepTemp) {
        Write-Host "Temp kept: $workDir"
    }
    elseif (Test-Path -LiteralPath $workDir) {
        Remove-Item -LiteralPath $workDir -Recurse -Force
    }
}
