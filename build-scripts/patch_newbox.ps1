$ErrorActionPreference = 'Stop'
$p = 'D:\kaifa-soft\_build\Sandboxie\SandboxiePlus\SandMan\Wizards\NewBoxWizard.cpp'
$lines = Get-Content -LiteralPath $p
Write-Host ("before: " + $lines.Count + " lines")

function Find-Index([string]$pattern, [int]$from = 0) {
    for ($i = $from; $i -lt $script:lines.Count; $i++) {
        if ($script:lines[$i] -match $pattern) { return $i }
    }
    return -1
}

# ---- 1) trim the box-type list to Standard + Application Compartment (Data Protection)
$firstHardened = Find-Index 'Security Hardened</a> Sandbox with'
$keepStart     = Find-Index 'AddBoxType\(tr\("Standard Sandbox"\)'
$keepEnd       = Find-Index 'ensuring smooth operation within the sandboxed environment\."\)\);' $keepStart
$appBoxPlain   = Find-Index '\(int\)CSandBoxPlus::eAppBox, '
$dropEnd       = $appBoxPlain + 2
if ($firstHardened -lt 0 -or $keepStart -lt 0 -or $keepEnd -lt 0 -or $appBoxPlain -lt 0) { throw "anchor not found" }
Write-Host ("hardened@" + ($firstHardened+1) + " keep " + ($keepStart+1) + ".." + ($keepEnd+1) + " dropEnd@" + ($dropEnd+1))

$kept = $lines[$keepStart..$keepEnd]
$lines = $lines[0..($firstHardened-1)] + $kept + $lines[($dropEnd+1)..($lines.Count-1)]

# ---- 2) confidential box checkbox: keep the field, never show it
$bb = Find-Index 'QWidget\* pBlackBox = AddBoxType'
if ($bb -lt 0) { throw "pBlackBox not found" }
Write-Host ("pBlackBox@" + ($bb+1))
$replacement = @(
'    // Local build: the encrypted / confidential box type is not offered, but the',
'    // wizard still reads the "blackBox" field, so keep an inert hidden checkbox.',
'    QCheckBox* pBlackBox = new QCheckBox();',
'    pBlackBox->setVisible(false);'
)
$lines = $lines[0..($bb-1)] + $replacement + $lines[($bb+4)..($lines.Count-1)]

Set-Content -LiteralPath $p -Value $lines
Write-Host ("after: " + (Get-Content -LiteralPath $p).Count + " lines")
