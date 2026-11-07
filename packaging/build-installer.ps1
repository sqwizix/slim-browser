param(
    [string]$BrowserExe = (Join-Path $PSScriptRoot '../artifacts/SlimBrowser-0.5.0-final/SlimBrowser-Rust.exe'),
    [string]$OutputDirectory = (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Slim Browser/Установщик')
)
$ErrorActionPreference = 'Stop'
$stage = Join-Path ([IO.Path]::GetTempPath()) ('SlimBrowser-package-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage -Force | Out-Null
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
Copy-Item -LiteralPath $BrowserExe -Destination (Join-Path $stage 'SlimBrowser.exe')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../rust/assets/ThirdParty.zip') -Destination $stage
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../rust/assets/SlimBrowser.ico') -Destination $stage
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../README.md') -Destination (Join-Path $stage 'README.md')
@'
.OPTION EXPLICIT
.Set CabinetNameTemplate=slim.cab
.Set MaxDiskSize=0
.Set MaxCabinetSize=0
.Set DiskDirectoryTemplate=.
.Set CompressionType=MSZIP
.Set Cabinet=on
.Set Compress=on
"SlimBrowser.exe" "BrowserExe"
"SlimBrowser.ico" "BrowserIcon"
"README.md" "BrowserReadme"
"ThirdParty.zip" "ThirdPartyZip"
'@ | Set-Content -LiteralPath (Join-Path $stage 'files.ddf') -Encoding ascii
Push-Location $stage
try { & makecab.exe /F files.ddf | Out-Null; if ($LASTEXITCODE -ne 0) { throw 'Cabinet build failed' } } finally { Pop-Location }
$cabinetBytes = [IO.File]::ReadAllBytes((Join-Path $stage 'slim.cab'))
if (([BitConverter]::ToUInt16($cabinetBytes, 30) -band 3) -ne 0) { throw 'Cabinet unexpectedly spans multiple disks' }
$installer = New-Object -ComObject WindowsInstaller.Installer
$msi = Join-Path $stage 'SlimBrowser-0.5.0-Setup.msi'
$database = $installer.OpenDatabase($msi, 3)
function Sql([string]$query) {
    $view = $database.OpenView($query)
    try { $view.Execute() } finally { $view.Close(); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($view) }
}
function Insert([string]$table, [string[]]$columns, [object[]]$values) {
    $columnSql = ($columns | ForEach-Object { '`' + $_ + '`' }) -join ','
    $placeholders = ($values | ForEach-Object { '?' }) -join ','
    $view = $database.OpenView("INSERT INTO ``$table`` ($columnSql) VALUES ($placeholders)")
    $record = $installer.CreateRecord($values.Count)
    for ($i=0; $i -lt $values.Count; $i++) {
        if ($values[$i] -is [int]) { $record.IntegerData($i+1) = $values[$i] }
        elseif ($null -ne $values[$i]) { $record.StringData($i+1) = [string]$values[$i] }
    }
    try { $view.Execute($record) } finally { $view.Close(); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($record); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($view) }
}
Sql 'CREATE TABLE `Property` (`Property` CHAR(72) NOT NULL, `Value` CHAR(0) LOCALIZABLE PRIMARY KEY `Property`)'
Sql 'CREATE TABLE `Directory` (`Directory` CHAR(72) NOT NULL, `Directory_Parent` CHAR(72), `DefaultDir` CHAR(255) NOT NULL LOCALIZABLE PRIMARY KEY `Directory`)'
Sql 'CREATE TABLE `Component` (`Component` CHAR(72) NOT NULL, `ComponentId` CHAR(38), `Directory_` CHAR(72) NOT NULL, `Attributes` SHORT NOT NULL, `Condition` CHAR(255), `KeyPath` CHAR(72) PRIMARY KEY `Component`)'
Sql 'CREATE TABLE `File` (`File` CHAR(72) NOT NULL, `Component_` CHAR(72) NOT NULL, `FileName` CHAR(255) NOT NULL LOCALIZABLE, `FileSize` LONG NOT NULL, `Version` CHAR(72), `Language` CHAR(20), `Attributes` SHORT, `Sequence` SHORT NOT NULL PRIMARY KEY `File`)'
Sql 'CREATE TABLE `Feature` (`Feature` CHAR(38) NOT NULL, `Feature_Parent` CHAR(38), `Title` CHAR(64) LOCALIZABLE, `Description` CHAR(255) LOCALIZABLE, `Display` SHORT, `Level` SHORT NOT NULL, `Directory_` CHAR(72), `Attributes` SHORT NOT NULL PRIMARY KEY `Feature`)'
Sql 'CREATE TABLE `FeatureComponents` (`Feature_` CHAR(38) NOT NULL, `Component_` CHAR(72) NOT NULL PRIMARY KEY `Feature_`, `Component_`)'
Sql 'CREATE TABLE `Media` (`DiskId` SHORT NOT NULL, `LastSequence` SHORT NOT NULL, `DiskPrompt` CHAR(64) LOCALIZABLE, `Cabinet` CHAR(255), `VolumeLabel` CHAR(32), `Source` CHAR(72) PRIMARY KEY `DiskId`)'
Sql 'CREATE TABLE `Shortcut` (`Shortcut` CHAR(72) NOT NULL, `Directory_` CHAR(72) NOT NULL, `Name` CHAR(128) NOT NULL LOCALIZABLE, `Component_` CHAR(72) NOT NULL, `Target` CHAR(255), `Arguments` CHAR(255), `Description` CHAR(255) LOCALIZABLE, `Hotkey` SHORT, `Icon_` CHAR(72), `IconIndex` SHORT, `ShowCmd` SHORT, `WkDir` CHAR(72) PRIMARY KEY `Shortcut`)'
Sql 'CREATE TABLE `Icon` (`Name` CHAR(72) NOT NULL, `Data` OBJECT NOT NULL PRIMARY KEY `Name`)'
Sql 'CREATE TABLE `Registry` (`Registry` CHAR(72) NOT NULL, `Root` SHORT NOT NULL, `Key` CHAR(255) NOT NULL LOCALIZABLE, `Name` CHAR(255) LOCALIZABLE, `Value` CHAR(0) LOCALIZABLE, `Component_` CHAR(72) NOT NULL PRIMARY KEY `Registry`)'
Sql 'CREATE TABLE `RemoveFile` (`FileKey` CHAR(72) NOT NULL, `Component_` CHAR(72) NOT NULL, `FileName` CHAR(255) LOCALIZABLE, `DirProperty` CHAR(72) NOT NULL, `InstallMode` SHORT NOT NULL PRIMARY KEY `FileKey`)'
Sql 'CREATE TABLE `InstallExecuteSequence` (`Action` CHAR(72) NOT NULL, `Condition` CHAR(255), `Sequence` SHORT PRIMARY KEY `Action`)'
Sql 'CREATE TABLE `InstallUISequence` (`Action` CHAR(72) NOT NULL, `Condition` CHAR(255), `Sequence` SHORT PRIMARY KEY `Action`)'
Sql 'CREATE TABLE `LaunchCondition` (`Condition` CHAR(255) NOT NULL, `Description` CHAR(255) NOT NULL LOCALIZABLE PRIMARY KEY `Condition`)'
foreach ($entry in @(
    @('ProductName','Slim Browser'), @('ProductVersion','0.5.0'), @('ProductLanguage','1033'),
    @('ProductCode','{19A3D522-A542-4D14-8AF2-E48977D6421E}'), @('UpgradeCode','{4CD682E3-217D-4D48-A463-C63C66E989D8}'),
    @('Manufacturer','Slim Browser'), @('ARPNOREPAIR','1'), @('ARPNOMODIFY','1'), @('ARPPRODUCTICON','SlimIcon'),
    @('MSIINSTALLPERUSER','1'), @('INSTALLLEVEL','1'), @('REINSTALLMODE','amus')
)) { Insert 'Property' @('Property','Value') $entry }
foreach ($entry in @(
    @('TARGETDIR',$null,'SourceDir'), @('LocalAppDataFolder','TARGETDIR','.'), @('ProgramsDir','LocalAppDataFolder','Programs'),
    @('INSTALLDIR','ProgramsDir','Slim Browser'), @('ProgramMenuFolder','TARGETDIR','.'), @('MenuDir','ProgramMenuFolder','Slim Browser'), @('DesktopFolder','TARGETDIR','.')
)) { Insert 'Directory' @('Directory','Directory_Parent','DefaultDir') $entry }
Insert 'Component' @('Component','ComponentId','Directory_','Attributes','KeyPath') @('Browser','{40FC1275-D52A-45D2-9CED-9B789831FF41}','INSTALLDIR',256,'BrowserExe')
$sequence = 0
foreach ($entry in @(@('BrowserExe','SlimBrowser.exe','0.5.0.0'),@('BrowserIcon','SlimBrowser.ico',$null),@('BrowserReadme','README.md',$null),@('ThirdPartyZip','ThirdParty.zip',$null))) {
    $sequence++
    Insert 'File' @('File','Component_','FileName','FileSize','Version','Attributes','Sequence') @($entry[0],'Browser',$entry[1],[int](Get-Item -LiteralPath (Join-Path $stage $entry[1])).Length,$entry[2],512,$sequence)
}
Insert 'Feature' @('Feature','Title','Description','Display','Level','Directory_','Attributes') @('Main','Slim Browser','Slim Browser for Windows',1,1,'INSTALLDIR',0)
Insert 'FeatureComponents' @('Feature_','Component_') @('Main','Browser')
Insert 'Media' @('DiskId','LastSequence','Cabinet') @(1,$sequence,'#slim.cab')
Insert 'Shortcut' @('Shortcut','Directory_','Name','Component_','Target','Description','Icon_','IconIndex','ShowCmd','WkDir') @('StartMenu','MenuDir','Slim Browser','Browser','[INSTALLDIR]SlimBrowser.exe','Slim Browser','SlimIcon',0,1,'INSTALLDIR')
Insert 'Registry' @('Registry','Root','Key','Name','Value','Component_') @('InstallLocation',1,'Software\SlimBrowser','InstallDir','[INSTALLDIR]','Browser')
Insert 'RemoveFile' @('FileKey','Component_','DirProperty','InstallMode') @('RemoveMenuFolder','Browser','MenuDir',2)
Insert 'LaunchCondition' @('Condition','Description') @('Installed OR VersionNT64','Slim Browser requires 64-bit Windows and Microsoft Edge WebView2 Runtime.')
foreach ($entry in @(
    @('LaunchConditions',100),@('CostInitialize',800),@('FileCost',900),@('CostFinalize',1000),@('InstallValidate',1400),@('InstallInitialize',1500),
    @('ProcessComponents',1600),@('UnpublishFeatures',1800),@('RemoveRegistryValues',2600),@('RemoveShortcuts',3200),@('RemoveFiles',3500),@('RemoveFolders',3600),
    @('CreateFolders',3700),@('InstallFiles',4000),@('CreateShortcuts',4500),@('WriteRegistryValues',5000),@('RegisterUser',6000),@('RegisterProduct',6100),
    @('PublishFeatures',6300),@('PublishProduct',6400),@('InstallFinalize',6600)
)) { Insert 'InstallExecuteSequence' @('Action','Sequence') $entry }
foreach ($entry in @(@('LaunchConditions',100),@('CostInitialize',800),@('FileCost',900),@('CostFinalize',1000),@('ExecuteAction',1300))) { Insert 'InstallUISequence' @('Action','Sequence') $entry }
function AddStream([string]$table,[string]$keyColumn,[string]$dataColumn,[string]$key,[string]$path) {
    $view = $database.OpenView("INSERT INTO ``$table`` (``$keyColumn``,``$dataColumn``) VALUES (?,?)")
    $record = $installer.CreateRecord(2); $record.StringData(1) = $key; $record.SetStream(2,$path)
    try { $view.Execute($record) } finally { $view.Close(); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($record); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($view) }
}
AddStream 'Icon' 'Name' 'Data' 'SlimIcon' (Join-Path $stage 'SlimBrowser.ico')
AddStream '_Streams' 'Name' 'Data' 'slim.cab' (Join-Path $stage 'slim.cab')
$summary = $database.SummaryInformation(20)
$summary.Property(2) = 'Slim Browser Setup'
$summary.Property(3) = 'Slim Browser 0.5.0'
$summary.Property(4) = 'Slim Browser'
$summary.Property(7) = 'x64;1033'
$summary.Property(9) = '{' + [guid]::NewGuid().ToString().ToUpper() + '}'
$summary.Property(14) = 200
$summary.Property(15) = 10
$summary.Property(19) = 2
$summary.Persist(); $database.Commit()
[void][Runtime.InteropServices.Marshal]::ReleaseComObject($summary)
[void][Runtime.InteropServices.Marshal]::ReleaseComObject($database)
[void][Runtime.InteropServices.Marshal]::ReleaseComObject($installer)
$output = Join-Path $OutputDirectory 'SlimBrowser-0.5.0-Setup.msi'
Copy-Item -LiteralPath $msi -Destination $output -Force
Write-Output "Installer: $output"
