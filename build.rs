//! Script de compilacion, sin dependencias:
//!  - flags de linker para un exe minimo sin CRT,
//!  - icono: convierte assets/blackscreen.ico en un .res (el formato binario que
//!    produce rc.exe) y se lo pasa al linker, asi no hace falta el Windows SDK,
//!  - manifiesto uiAccess opcional (feature "uiaccess").

use std::{env, fs, path::PathBuf};

const ICO: &str = "assets/blackscreen.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICO}");

    let link = |arg: &str| println!("cargo:rustc-link-arg-bins={arg}");

    // Sin CRT: el punto de entrada es mainCRTStartup de src/main.rs.
    // En debug, el `core` sin optimizar sigue pidiendo memset/memcpy/_fltused...,
    // asi que ahi se enlaza el CRT normal (nuestro mainCRTStartup tiene prioridad).
    if env::var("PROFILE").as_deref() == Ok("release") {
        link("/NODEFAULTLIB");
    } else {
        for lib in ["msvcrt", "vcruntime", "ucrt"] {
            println!("cargo:rustc-link-lib={lib}");
        }
    }
    // Ni PDB ni directorio de depuracion, ni cabecera "Rich" de version de herramientas.
    link("/DEBUG:NONE");
    link("/EMITTOOLVERSIONINFO:NO");
    link("/NOCOFFGRPINFO");
    // .pdata (tabla de unwind) es de solo lectura igual que .rdata: fusionarlas
    // ahorra el relleno de una seccion (512 B) sin cambiar ningun permiso.
    link("/MERGE:.pdata=.rdata");

    let res = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("icon.res");
    fs::write(&res, ico_to_res(&fs::read(ICO).expect("falta assets/blackscreen.ico"))).unwrap();
    link(res.to_str().unwrap());

    if env::var_os("CARGO_FEATURE_UIACCESS").is_some() {
        link("/MANIFEST:EMBED");
        // Un solo argumento: dos /MANIFESTUAC separados se pisan entre si.
        link("/MANIFESTUAC:level='asInvoker' uiAccess='true'");
    } else {
        link("/MANIFEST:NO");
    }
}

/// .ico -> .res con un RT_ICON por imagen y un RT_GROUP_ICON (id 1) que las agrupa.
fn ico_to_res(ico: &[u8]) -> Vec<u8> {
    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;
    let u16_at = |i: usize| u16::from_le_bytes([ico[i], ico[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes(ico[i..i + 4].try_into().unwrap());

    assert!(u16_at(0) == 0 && u16_at(2) == 1, "{ICO} no es un .ico");
    let count = u16_at(4);

    let mut res = Vec::new();
    entry(&mut res, 0, 0, 0, &[]); // cabecera nula obligatoria de los .res de 32 bits

    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&count.to_le_bytes());
    for n in 0..count {
        let e = 6 + 16 * n as usize; // ICONDIRENTRY
        let (size, offset) = (u32_at(e + 8) as usize, u32_at(e + 12) as usize);
        let id = n + 1;
        entry(&mut res, RT_ICON, id, 0x1010, &ico[offset..offset + size]);
        // GRPICONDIRENTRY = ICONDIRENTRY con el offset cambiado por un id de 16 bits.
        group.extend_from_slice(&ico[e..e + 12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    entry(&mut res, RT_GROUP_ICON, 1, 0x1030, &group);
    res
}

/// Una entrada RESOURCEHEADER con tipo y nombre numericos, idioma neutro.
fn entry(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    for v in [data.len() as u32, 32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in [0xFFFF, kind, 0xFFFF, name] {
        out.extend_from_slice(&u16::to_le_bytes(v));
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&flags.to_le_bytes()); // MemoryFlags
    out.extend_from_slice(&0u16.to_le_bytes()); // LanguageId (neutro)
    out.extend_from_slice(&[0; 8]); // Version + Characteristics
    out.extend_from_slice(data);
    out.resize(out.len().next_multiple_of(4), 0);
}
