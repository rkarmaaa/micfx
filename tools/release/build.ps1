# Build dell'installer da pubblicare (release su GitHub).
#
# Come `npm run build`, ma i percorsi della cartella utente (es. C:\Users\<nome>\.cargo\registry\...),
# che rustc scrive nei messaggi di panic delle dipendenze, diventano "~": l'eseguibile pubblico non
# contiene il nome utente di chi l'ha compilato. Il resto dei flag è quello di .cargo/config.toml
# (CARGO_ENCODED_RUSTFLAGS li sostituisce, non li somma: vanno ripetuti qui).
#
# Uso, dalla radice del repository:  powershell -File tools/release/build.ps1 [-OutDir <cartella>]
# (la versione per Mac: tools/release/build-macos.sh, su un Mac; le release su GitHub le fa
# .github/workflows/release.yml per entrambi i sistemi.)
param([string]$OutDir = '')
$ErrorActionPreference = 'Stop'

$sep = [char]0x1f
$env:CARGO_ENCODED_RUSTFLAGS = @(
  '-Ctarget-feature=+crt-static'
  "--remap-path-prefix=$($env:USERPROFILE)=~"
) -join $sep

npm run build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = Join-Path (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory 'release\micfx.exe'
$user = Split-Path $env:USERPROFILE -Leaf
if (Select-String -Path $exe -Pattern "\$user\" -SimpleMatch -Quiet) {
  Write-Error "micfx.exe contiene ancora '$user': non pubblicarlo."
}
Get-ChildItem (Join-Path (Split-Path $exe) 'bundle\nsis\*.exe') | ForEach-Object {
  "{0}  {1}" -f (Get-FileHash $_ -Algorithm SHA256).Hash.ToLower(), $_.Name
  if ($OutDir) {
    New-Item -ItemType Directory -Force $OutDir | Out-Null
    Copy-Item $_ $OutDir
  }
}
