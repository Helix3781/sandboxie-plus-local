# Build a proper two-tier test-signing PKI:
#   root CA (CA:true) -> code-signing leaf
# Signing with the leaf gives WinVerifyTrust a chain it accepts.
$ErrorActionPreference = 'Stop'

$outDir    = 'D:\kaifa-soft\_build\cert'
$rootCer   = Join-Path $outDir 'SandboxieLocalRootCA.cer'
$leafCer   = Join-Path $outDir 'SandboxieLocalCodeSigning.cer'
$leafPfx   = Join-Path $outDir 'SandboxieLocalCodeSigning.pfx'
$pwd       = ConvertTo-SecureString -String 'sandboxie-local' -Force -AsPlainText

# --- clean previous certs of both tiers from the user stores -----------------
Get-ChildItem 'Cert:\CurrentUser\My' -ErrorAction SilentlyContinue |
    Where-Object { $_.Subject -like '*Sandboxie Local*' } |
    ForEach-Object { Remove-Item $_.PSPath -Force }

# --- tier 1: root CA --------------------------------------------------------
$root = New-SelfSignedCertificate `
    -Subject 'CN=Sandboxie Local Root CA, O=Sandboxie Local Build, C=CN' `
    -Type Custom `
    -KeyUsage CertSign, CRLSign, DigitalSignature `
    -KeyUsageProperty Sign `
    -KeyExportPolicy Exportable `
    -KeyAlgorithm RSA -KeyLength 4096 -HashAlgorithm SHA256 `
    -NotAfter (Get-Date).AddYears(20) `
    -CertStoreLocation 'Cert:\CurrentUser\My' `
    -TextExtension @(
        '2.5.29.19={critical}{text}CA=true',
        '2.5.29.37={text}1.3.6.1.5.5.7.3.3'
    )
Export-Certificate -Cert $root -FilePath $rootCer -Type CERT | Out-Null
Write-Output ("root thumbprint: " + $root.Thumbprint)

# --- tier 2: code-signing leaf issued by the root ---------------------------
$leaf = New-SelfSignedCertificate `
    -Subject 'CN=Sandboxie Local Code Signing, O=Sandboxie Local Build, C=CN' `
    -Signer $root `
    -Type CodeSigningCert `
    -KeyExportPolicy Exportable `
    -KeyAlgorithm RSA -KeyLength 4096 -HashAlgorithm SHA256 `
    -NotAfter (Get-Date).AddYears(20) `
    -CertStoreLocation 'Cert:\CurrentUser\My'
Export-PfxCertificate -Cert $leaf -FilePath $leafPfx -Password $pwd | Out-Null
Export-Certificate   -Cert $leaf -FilePath $leafCer -Type CERT | Out-Null
Write-Output ("leaf thumbprint: " + $leaf.Thumbprint)

# --- trust the root for this user so we can verify locally ------------------
$st = New-Object System.Security.Cryptography.X509Certificates.X509Store('Root', 'CurrentUser')
$st.Open('ReadWrite'); $st.Add($root); $st.Close()
Write-Output 'root added to CurrentUser\Root'

Write-Output ('root BC : ' + (($root.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.19' }).Format($false) -replace '\s+', ' '))
Write-Output ('leaf BC : ' + (($leaf.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.19' }).Format($false) -replace '\s+', ' '))
Write-Output ('leaf EKU: ' + (($leaf.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' }).Format($false) -replace '\s+', ' '))
