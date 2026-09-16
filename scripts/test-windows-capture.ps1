# Cross-platform tests: parse and import function definitions only. Never run the
# Windows entrypoint. Registry calls are replaced by in-memory fakes.
param([string]$CaptureScript = (Join-Path $PSScriptRoot 'capture-windows-breathing.ps1'))
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
$tokens = $null; $parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($CaptureScript, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw ($parseErrors | Out-String) }
foreach ($definition in $ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $false)) {
    . ([scriptblock]::Create($definition.Extent.Text))
}
$script:Utf8 = New-Object Text.UTF8Encoding($false)
$script:Offsets = @{}
$script:Warnings = New-Object 'System.Collections.Generic.List[string]'
$script:LogSubkey = 'SOFTWARE\OEM\AcerLightingService\Log'
$script:LogValue = 'AcerLightingService'
$script:ServiceName = 'AcerLightingService'
$script:DriverDirectory = 'C:\Windows\System32\DriverStore\test'
function Assert-True($Value, [string]$Message) { if (-not $Value) { throw $Message } }
Invoke-SelfTest

$testDirectory = Join-Path ([IO.Path]::GetTempPath()) ('aps-capture-tests-' + [guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $testDirectory)
try {
    $fixture = 'LightingHID::SetLightingDeviceUpdateReport | 21,04,3C,03,01,00,00,FF,0F,00'
    $encodings = @([Text.Encoding]::ASCII, (New-Object Text.UTF8Encoding($true)), [Text.Encoding]::Unicode, [Text.Encoding]::BigEndianUnicode)
    $index = 0
    foreach ($encoding in $encodings) {
        $path = Join-Path $testDirectory ("encoding-$index.log")
        [IO.File]::WriteAllText($path, ($fixture + "`r`n"), $encoding)
        $read = Read-LogDelta (Get-Item -LiteralPath $path)
        Assert-True ($read.Contains($fixture)) "Encoding $index not decoded"
        Assert-True ((Read-LogDelta (Get-Item -LiteralPath $path)).Length -eq 0) 'Re-read old log data'
        [IO.File]::AppendAllText($path, ('APPENDED' + "`r`n"), $encoding)
        $tail = Read-LogDelta (Get-Item -LiteralPath $path)
        Assert-True ($tail.Contains('APPENDED') -and -not $tail.Contains($fixture)) 'Bad append offset'
        [IO.File]::WriteAllText($path, 'ROTATED', $encoding)
        Assert-True ((Read-LogDelta (Get-Item -LiteralPath $path)).Contains('ROTATED')) 'Truncation not handled'
        $index++
    }
    $script:Session = $testDirectory
    $script:LogRoots = @($testDirectory)
    $script:Offsets = @{}
    $records = @(Collect-Stage 'fixture')
    Assert-True ($records.Count -eq 0) 'Unrelated text became a hardware packet'
    Write-Host 'PASS: ASCII/UTF-8/UTF-16 log reads, append offsets, truncation, and collection filtering.'
} finally {
    # Only this test-created directory and its files are removed.
    Get-ChildItem -LiteralPath $testDirectory -File | Remove-Item
    Remove-Item -LiteralPath $testDirectory
}

$script:FakeValues = @{}
$script:FakeExists = $true
$script:FakeChildren = 0
$script:FakeKey = New-Object PSObject
$script:FakeKey | Add-Member ScriptMethod GetValueNames { return @($script:FakeValues.Keys) }
$script:FakeKey | Add-Member ScriptMethod GetValueKind { param($name) return [Microsoft.Win32.RegistryValueKind]::DWord }
$script:FakeKey | Add-Member ScriptMethod GetValue { param($name) return $script:FakeValues[$name] }
$script:FakeKey | Add-Member ScriptMethod SetValue { param($name, $value, $kind) $script:FakeValues[$name] = $value }
$script:FakeKey | Add-Member ScriptMethod DeleteValue { param($name, $ignored) $script:FakeValues.Remove($name) }
$script:FakeKey | Add-Member ScriptMethod Dispose { }
$script:FakeKey | Add-Member ScriptProperty ValueCount { return $script:FakeValues.Count }
$script:FakeKey | Add-Member ScriptProperty SubKeyCount { return $script:FakeChildren }
$script:FakeRoot = New-Object PSObject
$script:FakeRoot | Add-Member ScriptMethod OpenSubKey { param($path, $write) if ($script:FakeExists) { return $script:FakeKey }; return $null }
$script:FakeRoot | Add-Member ScriptMethod CreateSubKey { param($path) $script:FakeExists = $true; return $script:FakeKey }
$script:FakeRoot | Add-Member ScriptMethod DeleteSubKey { param($path, $ignored) $script:FakeExists = $false }
$script:FakeRoot | Add-Member ScriptMethod Dispose { }
function Open-Registry { return $script:FakeRoot }

$script:FakeValues['AcerLightingService'] = 7
$original = Get-LoggingBackup $true
Enable-Logging
Assert-True ($script:FakeValues['AcerLightingService'] -eq 1) 'Logging was not enabled'
Restore-Logging $original
Assert-True ($script:FakeValues['AcerLightingService'] -eq 7) 'Original DWORD not restored'
$script:FakeValues = @{}; $script:FakeExists = $false
$missing = Get-LoggingBackup $false
Enable-Logging
Restore-Logging $missing
Assert-True (-not $script:FakeExists) 'New empty Log key was not removed'
$script:FakeValues = @{}; $script:FakeExists = $false
$missing = Get-LoggingBackup $true
Enable-Logging
$script:FakeValues['ConcurrentOtherValue'] = 123
Restore-Logging $missing
Assert-True ($script:FakeExists -and $script:FakeValues['ConcurrentOtherValue'] -eq 123) 'Concurrent value was destroyed'
Assert-True (-not $script:FakeValues.ContainsKey('AcerLightingService')) 'New logging flag not removed'
$bad = [pscustomobject]@{ Version = 1; Subkey = 'SOFTWARE\Unrelated'; ValueName = 'AcerLightingService'; ServiceName = 'AcerLightingService' }
$rejected = $false
try { Restore-Logging $bad } catch { $rejected = $true }
Assert-True $rejected 'Unexpected recovery target was accepted'
Write-Host 'PASS: registry backup/restore, absent-key cleanup, concurrent-value preservation, and fixed recovery target.'
Write-Host 'PASS: script syntax and offline tests. Windows CIM/service/module operations still need live verification.'
