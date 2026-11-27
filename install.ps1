#Requires -RunAsAdministrator
<#
  Instala BlackScreen con uiAccess, para que tape tambien el menu Inicio,
  la barra de tareas y el Administrador de tareas.

  Windows solo concede uiAccess a un exe que este:
    1. firmado con un certificado en el que el equipo confie, y
    2. dentro de una ruta protegida (Program Files).

  Este script:
    - compila la variante uiaccess (si cargo esta disponible),
    - crea un certificado autofirmado y registra SOLO su parte publica como
      raiz de confianza del equipo,
    - firma el exe y BORRA la clave privada (nadie puede firmar nada mas con el),
    - copia el exe a "C:\Program Files\BlackScreen" y crea un acceso en Inicio.

  Desinstalar:  .\install.ps1 -Uninstall
#>
param([switch]$Uninstall)

$ErrorActionPreference = 'Stop'
$subject  = 'CN=BlackScreen Local'
$dest     = Join-Path $env:ProgramFiles 'BlackScreen'
$shortcut = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\BlackScreen.lnk'

function Remove-TrustedCert {
    Get-ChildItem Cert:\LocalMachine\Root | Where-Object Subject -eq $subject | Remove-Item
}

if ($Uninstall) {
    Remove-TrustedCert
    Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $shortcut -Force -ErrorAction SilentlyContinue
    Write-Host 'BlackScreen desinstalado.'
    return
}

$targetDir = Join-Path $PSScriptRoot 'target\uiaccess'
$exe       = Join-Path $targetDir 'release\blackscreen.exe'

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Push-Location $PSScriptRoot
    try {
        cargo build --release --features uiaccess --target-dir $targetDir
        if ($LASTEXITCODE -ne 0) { throw 'La compilacion fallo.' }
    } finally { Pop-Location }
} elseif (-not (Test-Path $exe)) {
    throw "No hay cargo en esta sesion ni exe precompilado en $exe. Compila antes con: cargo build --release --features uiaccess --target-dir target\uiaccess"
}

# Certificado nuevo en cada instalacion; el anterior deja de ser de confianza.
Remove-TrustedCert
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $subject `
    -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(20)
try {
    $public = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($cert.RawData)
    $store  = [System.Security.Cryptography.X509Certificates.X509Store]::new('Root', 'LocalMachine')
    $store.Open('ReadWrite'); $store.Add($public); $store.Close()

    $sig = Set-AuthenticodeSignature -FilePath $exe -Certificate $cert -HashAlgorithm SHA256
    if ($sig.Status -ne 'Valid') { throw "La firma no es valida: $($sig.StatusMessage)" }
} finally {
    Remove-Item $cert.PSPath -DeleteKey
}

Get-Process blackscreen -ErrorAction SilentlyContinue | Stop-Process -Force
New-Item $dest -ItemType Directory -Force | Out-Null
Copy-Item $exe $dest -Force

$lnk = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
$lnk.TargetPath = Join-Path $dest 'blackscreen.exe'
$lnk.Save()

Write-Host "Instalado en $dest. Abrelo desde el menu Inicio (BlackScreen); doble Esc para cerrar."
