param(
    [string]$BrowserExe = (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Slim Browser/SlimBrowser.exe'),
    [string]$ExpectedSha256 = '69D9198F38C1B52190C346A7C4DA1DF188AC14315D18EF0CBC1C02E6A9840073',
    [string]$OwnerSid = ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value),
    [switch]$ValidateOnly
)
$ErrorActionPreference='Stop'
$BrowserExe=[IO.Path]::GetFullPath($BrowserExe)
$directory=[IO.Path]::GetDirectoryName($BrowserExe)
$resultFile=Join-Path $directory 'system-registration-result.json'
try {
    if(-not [IO.File]::Exists($BrowserExe)){throw 'Browser executable is missing'}
    $hash=(Get-FileHash -LiteralPath $BrowserExe -Algorithm SHA256).Hash
    if($hash -ne $ExpectedSha256){throw 'Browser executable does not match the verified release'}
    if((Get-Item -LiteralPath $BrowserExe).VersionInfo.ProductName -ne 'Slim Browser'){throw 'Unexpected browser product'}
    $base='Software\Clients\StartMenuInternet\Slim Browser'
    $cap=$base+'\Capabilities'
    $command='"'+$BrowserExe+'" --open-url "%1"'
    $icon=$BrowserExe+',0'
    $values=New-Object 'System.Collections.Generic.List[object]'
    function Add-RegistrationValue([string]$path,[string]$name,[string]$data) {
        $values.Add([pscustomobject]@{Path=$path;Name=$name;Data=$data})
    }
    Add-RegistrationValue $base '' 'Slim Browser'
    Add-RegistrationValue ($base+'\DefaultIcon') '' $icon
    Add-RegistrationValue ($base+'\shell\open\command') '' ('"'+$BrowserExe+'"')
    Add-RegistrationValue $cap 'ApplicationName' 'Slim Browser'
    Add-RegistrationValue $cap 'ApplicationDescription' 'Slim Browser для веб-сайтов и ссылок'
    Add-RegistrationValue $cap 'ApplicationIcon' $icon
    foreach($scheme in @('http','https')) { Add-RegistrationValue ($cap+'\URLAssociations') $scheme 'SlimBrowser.Url' }
    foreach($extension in @('.htm','.html')) { Add-RegistrationValue ($cap+'\FileAssociations') $extension 'SlimBrowser.Html' }
    Add-RegistrationValue ($cap+'\Startmenu') 'StartMenuInternet' 'Slim Browser'
    Add-RegistrationValue 'Software\Microsoft\Windows\CurrentVersion\App Paths\SlimBrowser.exe' '' $BrowserExe
    Add-RegistrationValue 'Software\Classes\Applications\SlimBrowser.exe' 'FriendlyAppName' 'Slim Browser'
    Add-RegistrationValue 'Software\Classes\Applications\SlimBrowser.exe\shell\open\command' '' $command
    foreach($extension in @('.htm','.html')) { Add-RegistrationValue 'Software\Classes\Applications\SlimBrowser.exe\SupportedTypes' $extension '' }
    foreach($id in @('SlimBrowser.Html','SlimBrowser.Url')) {
        $class='Software\Classes\'+$id
        $title=if($id -eq 'SlimBrowser.Url'){'Slim Browser URL'}else{'Slim Browser HTML'}
        Add-RegistrationValue $class '' $title
        Add-RegistrationValue ($class+'\DefaultIcon') '' $icon
        Add-RegistrationValue ($class+'\shell\open\command') '' $command
        Add-RegistrationValue ($class+'\Application') 'ApplicationName' 'Slim Browser'
        Add-RegistrationValue ($class+'\Application') 'ApplicationIcon' $icon
        Add-RegistrationValue ($class+'\Application') 'ApplicationDescription' 'Slim Browser для веб-сайтов и ссылок'
        Add-RegistrationValue ($class+'\Application') 'AppUserModelId' 'SlimBrowser'
    }
    Add-RegistrationValue 'Software\Classes\SlimBrowser.Url' 'URL Protocol' ''
    Add-RegistrationValue 'Software\RegisteredApplications' 'Slim Browser' $cap
    if($values.Count -ne 32) {throw 'Incomplete registration plan'}
    foreach($entry in $values) {if(-not $entry.Path.StartsWith('Software\') -or $null -eq $entry.Name -or $null -eq $entry.Data){throw 'Invalid registration value'}}
    $plan=[ordered]@{executable=$BrowserExe;sha256=$hash;scope='LocalMachine';protocols=@('http','https');files=@('.htm','.html');registryValues=$values.Count}
    if($ValidateOnly){$plan|ConvertTo-Json;exit 0}
    $principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if(-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator confirmation is required for system registration'}
    $machine=[Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryView]::Registry64)
    $users=[Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::Users,[Microsoft.Win32.RegistryView]::Registry64)
    try {
        $existing=$machine.OpenSubKey('Software\Classes\SlimBrowser.Url\shell\open\command')
        if($existing){try{$old=[string]$existing.GetValue('');if($old -and $old -ne $command){throw 'Another system Slim Browser installation already exists'}}finally{$existing.Dispose()}}
        foreach($entry in $values){$key=$machine.CreateSubKey($entry.Path);try{$key.SetValue($entry.Name,$entry.Data,[Microsoft.Win32.RegistryValueKind]::String)}finally{$key.Dispose()}}
        # Remove duplicate app listings for this account; file/URL preferences stay with Windows.
        $registered=$users.OpenSubKey($OwnerSid+'\Software\RegisteredApplications',$true)
        if($registered){try{foreach($name in @('Slim Browser','SlimBrowser')){$data=[string]$registered.GetValue($name);if($data -eq $cap -or $data -eq 'Software\Clients\StartMenuInternet\SlimBrowser\Capabilities' -or $data -eq 'Software\SlimBrowser\Capabilities'){$registered.DeleteValue($name,$false)}}}finally{$registered.Dispose()}}
        foreach($id in @('SlimBrowser.Html','SlimBrowser.Url')){
            $relative=$OwnerSid+'\Software\Classes\'+$id
            $key=$users.OpenSubKey($relative+'\shell\open\command')
            if($key){try{$old=[string]$key.GetValue('')}finally{$key.Dispose()};if($old -and $old.StartsWith('"'+$directory+'\',[StringComparison]::OrdinalIgnoreCase)){$users.DeleteSubKeyTree($relative,$false)}}
        }
        foreach($relative in @('Software\Clients\StartMenuInternet\SlimBrowserPreview','Software\Classes\SlimBrowserPreviewURL','Software\Classes\SlimBrowserPreviewHTML')) {
            $key=$machine.OpenSubKey($relative+'\shell\open\command')
            if($key){try{$old=[string]$key.GetValue('')}finally{$key.Dispose()};if($old -eq $command -or $old -eq ('"'+$BrowserExe+'"')){$machine.DeleteSubKeyTree($relative,$false)}}
        }
        $registered=$machine.OpenSubKey('Software\RegisteredApplications',$true)
        try{if([string]$registered.GetValue('SlimBrowserPreview') -eq 'Software\Clients\StartMenuInternet\SlimBrowserPreview\Capabilities'){$registered.DeleteValue('SlimBrowserPreview',$false)}}finally{$registered.Dispose()}
    }finally{$machine.Dispose();$users.Dispose()}
    Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;public static class SlimRegistrationNotify{[DllImport("shell32.dll")] public static extern void SHChangeNotify(uint e,uint f,IntPtr a,IntPtr b);}'
    [SlimRegistrationNotify]::SHChangeNotify(0x08000000,0x3000,[IntPtr]::Zero,[IntPtr]::Zero)
    $plan['success']=$true
    $plan|ConvertTo-Json|Set-Content -LiteralPath $resultFile -Encoding UTF8
    exit 0
}catch{
    @{success=$false;error=$_.Exception.Message;executable=$BrowserExe}|ConvertTo-Json|Set-Content -LiteralPath $resultFile -Encoding UTF8
    exit 1
}
