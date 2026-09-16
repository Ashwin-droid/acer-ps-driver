#requires -Version 5.1
<#
Run in 64-bit Windows PowerShell as Administrator. No downloads, drivers,
kernel debugging, Secure Boot changes, or raw firmware writes.
Only Acer's logging DWORD and its lighting service are changed automatically.
You apply lighting through PredatorSense. Logging/service state is restored.
Windows lighting choices are left as you select them; before/after recipes are saved.
#>
[CmdletBinding()]
param(
    [string]$OutputDirectory = '',
    [switch]$RestoreOnly,
    [string]$SessionDirectory = '',
    [switch]$SelfTest
)
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
$script:LogSubkey = 'SOFTWARE\OEM\AcerLightingService\Log'
$script:LogValue = 'AcerLightingService'
$script:ServiceName = 'AcerLightingService'
$script:Offsets = @{}
$script:Stages = New-Object 'System.Collections.Generic.List[object]'
$script:Warnings = New-Object 'System.Collections.Generic.List[string]'
$script:LoadedKeyboardSeen = $false
$script:Utf8 = New-Object System.Text.UTF8Encoding($false)

function Write-Utf8([string]$Path, [string]$Text) {
    [IO.File]::WriteAllText($Path, $Text, $script:Utf8)
}
function Write-Json([string]$Path, $Value) {
    Write-Utf8 $Path (ConvertTo-Json -InputObject $Value -Depth 20)
}
function Get-Field($Value, [string]$Name) {
    if ($null -ne $Value) {
        $property = $Value.PSObject.Properties[$Name]
        if ($null -ne $property) { return $property.Value }
    }
    return $null
}
function Add-Warning([string]$Message) {
    $script:Warnings.Add($Message)
    Write-Warning $Message
}
function Parse-AcerLine([string]$Line) {
    # Raw payload logging omits A4 itself but prints ten payload bytes.
    # Restrict inference to the exact serializer's marker and keyboard target.
    if ($Line -match 'SetLightingDeviceUpdateReport') {
        $matches = [regex]::Matches($Line, '(?i)(?<![0-9a-f])(?:[0-9a-f]{2},\s*){9}[0-9a-f]{2}(?![0-9a-f])')
        foreach ($match in $matches) {
            $b = @($match.Value -split ',\s*' | ForEach-Object { [Convert]::ToInt32($_, 16) })
            $zones = $b[8] + 256 * $b[9]
            if ($b[0] -eq 0x21 -and $b[1] -ge 1 -and $b[1] -le 31 -and
                $b[2] -le 100 -and $zones -ge 1 -and $zones -le 15) {
                return [pscustomobject]@{
                    Kind = 'serializer-payload'; Effect = $b[1]; Brightness = $b[2]
                    Speed = $b[3]; Direction = $b[4]; Zones = $zones
                    RGB = ('{0:X2}{1:X2}{2:X2}' -f $b[5], $b[6], $b[7])
                    InferredReport = ('A4' + (($b | ForEach-Object { '{0:X2}' -f $_ }) -join ''))
                    Source = $Line
                }
            }
        }
    }
    if ($Line -match 'AcerHIDKeyboardController::SetMode' -and
        $Line -match '(?i)mode=(\d+),\s*speed=(\d+),\s*direction=(\d+),\s*brightness=(\d+),\s*color=\s*\(0x([0-9a-f]{2}),\s*0x([0-9a-f]{2}),\s*0x([0-9a-f]{2})\)') {
        return [pscustomobject]@{
            Kind = 'mode-arguments-only'; Effect = [int]$Matches[1]; Speed = [int]$Matches[2]
            Direction = [int]$Matches[3]; Brightness = [int]$Matches[4]
            RGB = ($Matches[5] + $Matches[6] + $Matches[7]).ToUpperInvariant()
            Source = $Line
        }
    }
    return $null
}
function Test-RawColor($Records, [int]$Effect, [string]$RGB) {
    return @($Records | Where-Object {
        $_.Kind -eq 'serializer-payload' -and $_.Effect -eq $Effect -and $_.RGB -eq $RGB
    }).Count -gt 0
}
function Invoke-SelfTest {
    $red = Parse-AcerLine 'LightingHID::SetLightingDeviceUpdateReport | 21,02,3C,00,00,FF,00,00,0F,00'
    if (-not (Test-RawColor @($red) 2 'FF0000')) { throw 'Static-red packet test failed' }
    $blue = Parse-AcerLine 'LightingHID::SetLightingDeviceUpdateReport | 21,04,3C,03,01,00,00,FF,0F,00'
    if (-not (Test-RawColor @($blue) 4 '0000FF')) { throw 'Blue breathing packet test failed' }
    foreach ($bad in @(
        'SetLightingDeviceUpdateReport | 65,02,3C,00,00,FF,00,00,0F,00',
        'SetLightingDeviceUpdateReport | 21,02,FF,00,00,FF,00,00,0F,00',
        'SetLightingDeviceUpdateReport | 21,02,3C,00,00,FF,00,00,FF,FF',
        'SetLightingDeviceUpdateReport | 21,02,3C,00,00,FF,00,00,0F',
        'unrelated | 21,02,3C,00,00,FF,00,00,0F,00'
    )) { if ($null -ne (Parse-AcerLine $bad)) { throw "Accepted invalid packet: $bad" } }
    $argsOnly = Parse-AcerLine 'AcerHIDKeyboardController::SetMode | mode=4, speed=3, direction=1, brightness=60, color=(0x00,0x00,0xFF) zone=0F'
    if ($null -eq $argsOnly -or (Test-RawColor @($argsOnly) 4 '0000FF')) { throw 'Mode arguments mislabeled as raw packet' }
    if (Test-RawColor @() 4 '0000FF') { throw 'Empty capture passed' }
    Write-Host 'PASS: packet parser, color matching, malformed/other-target rejection, and evidence grading.'
}
if ($SelfTest) { Invoke-SelfTest; return }

function Open-Registry {
    return [Microsoft.Win32.RegistryKey]::OpenBaseKey(
        [Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry64)
}
function Get-LoggingBackup([bool]$WasRunning) {
    $root = Open-Registry
    $key = $null
    try {
        $key = $root.OpenSubKey($script:LogSubkey, $false)
        $exists = $null -ne $key
        $valueExists = $exists -and ($key.GetValueNames() -contains $script:LogValue)
        $original = $null
        if ($valueExists) {
            if ($key.GetValueKind($script:LogValue) -ne [Microsoft.Win32.RegistryValueKind]::DWord) {
                throw 'Unexpected logging value type; refusing to change it.'
            }
            $original = [int]$key.GetValue($script:LogValue)
        }
        return [pscustomobject]@{
            Version = 1; Subkey = $script:LogSubkey; ValueName = $script:LogValue
            KeyExisted = $exists; ValueExisted = $valueExists; OriginalValue = $original
            ServiceName = $script:ServiceName; ServiceWasRunning = $WasRunning
            DriverDirectory = $script:DriverDirectory
        }
    } finally { if ($null -ne $key) { $key.Dispose() }; $root.Dispose() }
}
function Enable-Logging {
    $root = Open-Registry; $key = $null
    try {
        $key = $root.CreateSubKey($script:LogSubkey)
        $key.SetValue($script:LogValue, [int]1, [Microsoft.Win32.RegistryValueKind]::DWord)
    } finally { if ($null -ne $key) { $key.Dispose() }; $root.Dispose() }
}
function Restore-Logging($Backup) {
    if ($Backup.Version -ne 1 -or $Backup.Subkey -ne $script:LogSubkey -or
        $Backup.ValueName -ne $script:LogValue -or $Backup.ServiceName -ne $script:ServiceName) {
        throw 'Recovery record does not describe the fixed Acer logging setting.'
    }
    $root = Open-Registry; $key = $null
    try {
        $key = $root.OpenSubKey($script:LogSubkey, $true)
        if ($Backup.ValueExisted) {
            if ($null -eq $key) { $key = $root.CreateSubKey($script:LogSubkey) }
            $key.SetValue($script:LogValue, [int]$Backup.OriginalValue, [Microsoft.Win32.RegistryValueKind]::DWord)
        } elseif ($null -ne $key) {
            $key.DeleteValue($script:LogValue, $false)
        }
        # Remove only the exact empty key we created, never a tree or another value.
        if (-not $Backup.KeyExisted -and $null -ne $key -and
            $key.ValueCount -eq 0 -and $key.SubKeyCount -eq 0) {
            $key.Dispose(); $key = $null
            $root.DeleteSubKey($script:LogSubkey, $false)
        }
    } finally { if ($null -ne $key) { $key.Dispose() }; $root.Dispose() }
}
function Get-AcerProcesses {
    return @(Get-CimInstance Win32_Process -Filter "Name='OpenRGB.exe' OR Name='AcerLightingService.exe' OR Name='AcerAgentService.exe'" |
        Where-Object { $_.ExecutablePath -and ([IO.Path]::GetDirectoryName($_.ExecutablePath) -ieq $script:DriverDirectory) })
}
function Restart-AcerLighting([bool]$RunAfter) {
    $service = Get-Service -Name $script:ServiceName
    if ($service.Status -ne 'Stopped') {
        Stop-Service -Name $script:ServiceName -ErrorAction Stop
        (Get-Service $script:ServiceName).WaitForStatus('Stopped', [TimeSpan]::FromSeconds(20))
    }
    # Stop only Acer's exact sibling OpenRGB, not a separately installed OpenRGB.
    foreach ($process in @(Get-AcerProcesses | Where-Object { $_.Name -ieq 'OpenRGB.exe' })) {
        $live = Get-Process -Id $process.ProcessId -ErrorAction SilentlyContinue
        if ($null -ne $live -and $live.Path -ieq (Join-Path $script:DriverDirectory 'OpenRGB.exe')) {
            Stop-Process -Id $process.ProcessId -ErrorAction Stop
        }
    }
    if ($RunAfter) {
        Start-Service -Name $script:ServiceName
        (Get-Service $script:ServiceName).WaitForStatus('Running', [TimeSpan]::FromSeconds(20))
        Start-Sleep -Seconds 3
    }
}
function Save-Inventory([string]$Label) {
    $items = @()
    foreach ($process in @(Get-AcerProcesses)) {
        $modules = @(); $moduleError = $null
        try {
            $modules = @((Get-Process -Id $process.ProcessId).Modules | Where-Object {
                $_.ModuleName -in @('AcerECKeyboardController.dll', 'AcerECLightbarController.dll', 'hid.dll')
            } | ForEach-Object {
                [pscustomobject]@{ Name = $_.ModuleName; Path = $_.FileName
                    Version = $_.FileVersionInfo.FileVersion; SHA256 = (Get-FileHash -LiteralPath $_.FileName -Algorithm SHA256).Hash }
            })
        } catch { $moduleError = $_.Exception.Message }
        if (@($modules | Where-Object { $_.Name -ieq 'AcerECKeyboardController.dll' }).Count -gt 0) {
            $script:LoadedKeyboardSeen = $true
        }
        $items += [pscustomobject]@{ Name = $process.Name; PID = $process.ProcessId
            ParentPID = $process.ParentProcessId; Path = $process.ExecutablePath
            Modules = $modules; ModuleReadError = $moduleError }
    }
    Write-Json (Join-Path $script:Session ($Label + '-processes.json')) @($items)
}
function Snapshot-Lighting([string]$Label) {
    $ini = Join-Path $env:ProgramData 'OEM\AcerLightingService\LightingProfile\LightingProfile.ini'
    if (Test-Path -LiteralPath $ini -PathType Leaf) {
        Copy-Item -LiteralPath $ini -Destination (Join-Path $script:Session ($Label + '-LightingProfile.ini'))
    }
    $config = Join-Path $env:ProgramData 'OEM\PredatorSense\ProfilePool\config.json'
    if (Test-Path -LiteralPath $config -PathType Leaf) {
        try {
            $data = Get-Content -LiteralPath $config -Raw | ConvertFrom-Json
            $scenario = Get-Field $data 'Scenario'
            $lighting = @(); $index = 0
            foreach ($profile in @(Get-Field $scenario 'profiles')) {
                $lighting += [pscustomobject]@{ Index = $index; Lighting = (Get-Field $profile 'lighting') }
                $index++
            }
            # Deliberately omit UUIDs, app history, account data and other settings.
            Write-Json (Join-Path $script:Session ($Label + '-scenario-lighting.json')) ([pscustomobject]@{
                Active = (Get-Field $scenario 'active'); Profiles = $lighting
            })
        } catch { Add-Warning "Could not snapshot lighting-only scenario: $($_.Exception.Message)" }
    }
}
function Get-LogFiles {
    $result = @()
    foreach ($root in $script:LogRoots) {
        if (Test-Path -LiteralPath $root -PathType Container) {
            $result += @(Get-ChildItem -LiteralPath $root -File -Filter '*.log' -ErrorAction SilentlyContinue)
        }
    }
    return @($result | Sort-Object FullName -Unique)
}
function Mark-LogBoundary {
    foreach ($file in @(Get-LogFiles)) { $script:Offsets[$file.FullName] = [long]$file.Length }
}
function Read-LogDelta([IO.FileInfo]$File) {
    $stream = $null; $reader = $null
    try {
        $share = [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete
        $stream = New-Object IO.FileStream($File.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, $share)
        $offset = [long]0
        if ($script:Offsets.ContainsKey($File.FullName)) { $offset = $script:Offsets[$File.FullName] }
        if ($stream.Length -lt $offset) { $offset = 0 }
        if ($stream.Length - $offset -gt 16MB) { throw 'New log data exceeds 16 MB; capture is incomplete.' }
        $head = New-Object byte[] 128
        $n = $stream.Read($head, 0, $head.Length)
        $encoding = [Text.Encoding]::Default
        if ($n -ge 2 -and $head[0] -eq 255 -and $head[1] -eq 254) { $encoding = [Text.Encoding]::Unicode }
        elseif ($n -ge 2 -and $head[0] -eq 254 -and $head[1] -eq 255) { $encoding = [Text.Encoding]::BigEndianUnicode }
        elseif ($n -ge 3 -and $head[0] -eq 239 -and $head[1] -eq 187 -and $head[2] -eq 191) { $encoding = [Text.Encoding]::UTF8 }
        elseif ($n -ge 4 -and $head[1] -eq 0 -and $head[3] -eq 0) { $encoding = [Text.Encoding]::Unicode }
        [void]$stream.Seek($offset, [IO.SeekOrigin]::Begin)
        $reader = New-Object IO.StreamReader($stream, $encoding, $true)
        $text = $reader.ReadToEnd()
        $script:Offsets[$File.FullName] = $stream.Position
        return $text
    } finally { if ($null -ne $reader) { $reader.Dispose() }; if ($null -ne $stream) { $stream.Dispose() } }
}
function Collect-Stage([string]$Name) {
    $records = @(); $ordinal = 0
    foreach ($file in @(Get-LogFiles)) {
        try {
            $text = Read-LogDelta $file
            if ($text.Length -eq 0) { continue }
            $ordinal++
            $destination = Join-Path $script:Session ('{0}-{1:D2}-{2}' -f $Name, $ordinal, $file.Name)
            Write-Utf8 $destination $text
            foreach ($line in ($text -split '\r?\n')) {
                $record = Parse-AcerLine $line
                if ($null -ne $record) { $records += $record }
            }
        } catch { Add-Warning "Could not collect $($file.Name): $($_.Exception.Message)" }
    }
    return $records
}
function Run-Stage([string]$Name, [string]$Instructions) {
    Write-Host "`n=== $Name ===" -ForegroundColor Cyan
    Write-Host $Instructions
    Mark-LogBoundary
    $started = (Get-Date).ToString('o')
    [void](Read-Host 'Apply, watch about 15 seconds (at least two cycles for Breathing), then press Enter (Q aborts)' | ForEach-Object {
        if ($_ -eq 'Q') { throw 'Aborted by user; cleanup will run.' }
    })
    $observation = Read-Host 'What did the keyboard actually do? (e.g. steady red, red-only breathing, RGB cycling, no Random toggle)'
    Start-Sleep -Seconds 2
    $records = @(Collect-Stage $Name)
    Snapshot-Lighting $Name
    Save-Inventory $Name
    $stage = [pscustomobject]@{ Name = $Name; Start = $started; End = (Get-Date).ToString('o')
        Observation = $observation; Records = $records
        RawPayloads = @($records | Where-Object { $_.Kind -eq 'serializer-payload' }).Count
        ModeArguments = @($records | Where-Object { $_.Kind -eq 'mode-arguments-only' }).Count }
    $script:Stages.Add($stage)
    Write-Json (Join-Path $script:Session 'stages.json') @($script:Stages.ToArray())
    Write-Host ('Captured {0} keyboard payloads and {1} mode-argument entries.' -f $stage.RawPayloads, $stage.ModeArguments)
    return $stage
}

# No Windows-only command runs in -SelfTest mode.
if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitProcess) {
    throw 'Run this script in 64-bit Windows PowerShell, not Linux or PowerShell (x86).'
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Open Windows Terminal / Windows PowerShell as Administrator, then run the command in RUN-ME.txt.'
}
$serviceInfo = Get-CimInstance Win32_Service -Filter "Name='AcerLightingService'"
if ($null -eq $serviceInfo) { throw 'AcerLightingService is not installed.' }
if ($serviceInfo.State -notin @('Running', 'Stopped')) { throw 'Acer lighting service is transitioning; wait a moment and run the script again.' }
$servicePath = [Environment]::ExpandEnvironmentVariables($serviceInfo.PathName).Trim()
if ($servicePath -match '^"([^"]+)"(?:\s.*)?$') { $servicePath = $Matches[1] }
if ([IO.Path]::GetFileName($servicePath) -ine 'AcerLightingService.exe' -or -not (Test-Path -LiteralPath $servicePath -PathType Leaf)) {
    throw 'Unexpected Acer service image path; no settings changed.'
}
$script:DriverDirectory = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($servicePath))
if ($RestoreOnly) {
    if (-not $SessionDirectory) { throw '-RestoreOnly requires -SessionDirectory from a previous capture.' }
    $backup = Get-Content -LiteralPath (Join-Path $SessionDirectory 'logging-backup.json') -Raw | ConvertFrom-Json
    Restore-Logging $backup
    Restart-AcerLighting ([bool]$backup.ServiceWasRunning)
    Write-Host 'Acer logging and lighting-service state restored. No lighting profile file was overwritten.'
    return
}
$computer = Get-CimInstance Win32_ComputerSystem
$bios = Get-CimInstance Win32_BIOS
if ($computer.Manufacturer -notmatch '^Acer' -or $computer.Model.Trim() -ne 'Predator PHN16S-71') {
    throw 'This capture is scoped to the Predator PHN16S-71. No settings changed.'
}
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $PSScriptRoot 'captures' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if ($OutputDirectory.StartsWith('\\')) { throw 'Choose a local output directory, not a network share.' }
$script:Session = Join-Path $OutputDirectory ((Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
[void](New-Item -ItemType Directory -Path $script:Session)
$script:LogRoots = @(
    (Join-Path $env:ProgramData 'OEM\AcerLightingService\Log'),
    $script:DriverDirectory,
    (Join-Path $script:DriverDirectory 'logs')
)
$backup = Get-LoggingBackup ($serviceInfo.State -eq 'Running')
Write-Json (Join-Path $script:Session 'logging-backup.json') $backup
Write-Json (Join-Path $script:Session 'machine.json') ([pscustomobject]@{
    Model = $computer.Model; Manufacturer = $computer.Manufacturer; BIOS = $bios.SMBIOSBIOSVersion
    PowerShell = $PSVersionTable.PSVersion.ToString(); ServiceImage = $servicePath
    ServiceVersion = (Get-Item -LiteralPath $servicePath).VersionInfo.FileVersion
    ServiceSHA256 = (Get-FileHash -LiteralPath $servicePath -Algorithm SHA256).Hash
    Started = (Get-Date).ToString('o')
})
Write-Utf8 (Join-Path $script:Session 'RECOVER.txt') @"
If this window was killed or Windows crashed, run as Administrator:
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PSCommandPath" -RestoreOnly -SessionDirectory "$script:Session"
This restores only Acer's logging value and original lighting-service state.
"@
Snapshot-Lighting 'before'
Save-Inventory 'before'
$changed = $false; $failure = $null; $cleanupOk = $true; $transcript = $false
$rawComplete = $false
try {
    Start-Transcript -LiteralPath (Join-Path $script:Session 'console.txt') | Out-Null
    $transcript = $true
    Write-Host "Capture folder: $script:Session" -ForegroundColor Green
    Write-Host 'This temporarily enables Acer logging and restarts only the Acer lighting service/helper.'
    Write-Host 'It does not change Secure Boot, install anything, or send raw firmware commands.'
    Write-Host 'You change KEYBOARD lighting in PredatorSense. Do not change logo, fan, performance or display controls.'
    if ((Read-Host 'Press Enter to begin, or Q to cancel') -eq 'Q') { throw 'Cancelled before logging was enabled.' }
    Mark-LogBoundary
    $changed = $true  # Set before the registry write so partial failure is recoverable.
    Enable-Logging
    Restart-AcerLighting $true
    Write-Host 'Open PredatorSense now and select the KEYBOARD lighting page.'
    $baseline = Run-Stage '01-static-red' 'Choose Static. Set BLUE briefly, then RED (#FF0000) and apply. Use a comfortable brightness and leave it unchanged for all later steps.'
    while (-not (Test-RawColor $baseline.Records 2 'FF0000')) {
        Write-Warning 'No trustworthy Static-red keyboard payload was found. Mode messages alone are not a raw capture.'
        $answer = Read-Host 'R = retry Static red, C = continue with PARTIAL evidence, Q = stop and restore logging'
        if ($answer -eq 'R') { $baseline = Run-Stage '01-static-red-retry' 'Switch Static BLUE, then RED (#FF0000), and apply.' }
        elseif ($answer -eq 'C') { Add-Warning 'Continuing without a verified raw Static-red baseline.'; break }
        else { throw 'Stopped because the raw capture baseline was not verified.' }
    }
    $random = Run-Stage '02-breathing-random' 'Choose Breathing. Enable Random / multicolor if available. Choose a moderate speed and keep it unchanged afterward. If no Random control exists, record that.'
    $red = Run-Stage '03-breathing-fixed-red' 'Keep Breathing, brightness and speed unchanged. Turn Random OFF and select RED (#FF0000). Apply. If this is impossible or still cycles, say so explicitly.'
    $blue = Run-Stage '04-breathing-fixed-blue' 'Keep Random OFF, Breathing, brightness and speed unchanged. Change only the selected color to BLUE (#0000FF) and apply.'
    $rawComplete = (Test-RawColor $baseline.Records 2 'FF0000') -and
        (Test-RawColor $red.Records 4 'FF0000') -and (Test-RawColor $blue.Records 4 '0000FF') -and
        (@($random.Records | Where-Object { $_.Kind -eq 'serializer-payload' -and $_.Effect -eq 4 }).Count -gt 0)
    Write-Host "`nIf you want your original Windows lighting back, restore it in PredatorSense now."
    [void](Read-Host 'Press Enter when finished. The script will NOT overwrite Windows lighting profile files')
} catch {
    $failure = $_.Exception.Message
    Write-Warning $failure
} finally {
    try { $null = @(Collect-Stage 'cleanup-before-restore') } catch { Add-Warning $_.Exception.Message }
    if ($changed) {
        try { Restore-Logging $backup } catch { $cleanupOk = $false; Add-Warning ('LOGGING RESTORE FAILED: ' + $_.Exception.Message) }
        try { Restart-AcerLighting ([bool]$backup.ServiceWasRunning) } catch { $cleanupOk = $false; Add-Warning ('SERVICE RESTORE FAILED: ' + $_.Exception.Message) }
    }
    try { Snapshot-Lighting 'after'; Save-Inventory 'after' } catch { Add-Warning $_.Exception.Message }
    Write-Json (Join-Path $script:Session 'stages.json') @($script:Stages.ToArray())
    Write-Json (Join-Path $script:Session 'result.json') ([pscustomobject]@{
        RawComparisonsPresent = $rawComplete; CleanupSucceeded = $cleanupOk
        LoadedKeyboardModuleObserved = $script:LoadedKeyboardSeen
        Failure = $failure; Warnings = @($script:Warnings.ToArray())
        Note = 'Serializer logs are not a bus trace. Visible fixed-color success must come from observations. No firmware support claim is inferred automatically.'
    })
    if ($transcript) { Stop-Transcript | Out-Null }
}
$summary = @(
    'APS Windows native Breathing capture',
    ('Raw comparison packets present: ' + $rawComplete),
    ('Logging/service cleanup succeeded: ' + $cleanupOk),
    ('Loaded keyboard DLL recorded: ' + $script:LoadedKeyboardSeen),
    ('Failure: ' + $failure),
    '', 'Visual observations:'
)
foreach ($stage in $script:Stages) { $summary += ($stage.Name + ': ' + $stage.Observation) }
$summary += @('', 'A raw-comparison pass means useful serializer records were found, NOT that fixed-color Breathing works.')
if (-not $rawComplete) {
    $summary += @('PARTIAL CAPTURE: raw records are missing or do not match the chosen colors.',
        'Do not treat this as a complete packet trace. Keep these files; mode logs and visual observations may still be useful.',
        'If fixed-color Breathing DOES work visually, stay in Windows and send result.json/SUMMARY.txt before rebooting.',
        'The next capture method is user-mode WinDbg on the Acer process loading AcerECKeyboardController.dll, at hid!HidD_SetFeature.',
        'Do not install a USB filter driver or disable Secure Boot for this I2C keyboard.')
}
if (-not $cleanupOk) { $summary += 'IMPORTANT: run the command in RECOVER.txt as Administrator.' }
if (-not $script:LoadedKeyboardSeen) { $summary += 'WARNING: running keyboard DLL could not be identified; include the process inventory errors with the capture.' }
Write-Utf8 (Join-Path $script:Session 'SUMMARY.txt') ($summary -join [Environment]::NewLine)
try {
    $zip = $script:Session + '.zip'
    Compress-Archive -LiteralPath $script:Session -DestinationPath $zip -CompressionLevel Optimal
    Write-Host "`nSaved capture: $zip" -ForegroundColor Green
} catch { Write-Warning "ZIP creation failed; the capture folder is intact: $script:Session" }
Write-Host ($summary -join [Environment]::NewLine)
Write-Host "`nKeep this capture folder. Back on Linux, I can read it from the mounted Windows drive."
