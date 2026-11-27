# BlackScreen

Pantalla negra a pantalla completa (todos los monitores) que simula el monitor apagado. Mantiene el equipo despierto mientras está abierta. **Doble Esc** para salir.

Win32 puro en Rust, `no_std`, sin dependencias: el exe pesa ~6 KB.

```powershell
cargo build --release
```

Para que tape también el menú Inicio y el Administrador de tareas, instálala con `uiAccess` (PowerShell como administrador): `.\install.ps1`. Para desinstalar: `.\install.ps1 -Uninstall`.

Licencia MIT.
