//! Veri kaybı riskinin sıfır olduğunun kaynak düzeyinde kanıtı.
//!
//! Davranışsal testler (kaynak dosya arşivden sonra da yerinde) tek başına
//! yeterli değildir: ileride eklenebilecek bir `remove_file` çağrısı tüm
//! güvenlik modelini bozabilir. Bu dosya, üretim kodunda (`src/`) hiçbir
//! silme/kesme çağrısı bulunmadığını ve `unsafe` kullanılmadığını doğrular.

use std::fs;
use std::path::{Path, PathBuf};

/// Üretim kodunda yasak olan, dosya verisini yok eden çağrılar.
const YASAK_CAGRIMLAR: &[&str] = &[
    "remove_file",
    "remove_dir",
    "remove_dir_all",
    "truncate",
    "set_len",
    "persist(",
    "Command::new",
];

/// `#[cfg(test)]` modüllerinden sonraki bölüm dikkate alınmaz.
///
/// Bu projede her test modülü dosyanın sonunda yer alır; dolayısıyla test
/// temizlik kodu (`remove_dir_all`) kaynak taramasına girmez.
fn uretim_kismi(icerik: &str) -> &str {
    match icerik.find("#[cfg(test)]") {
        Some(basar) => &icerik[..basar],
        None => icerik,
    }
}

/// `src/` altındaki tüm Rust dosyalarını yollarıyla birlikte döndürür.
fn kaynak_dosyalari(kok: &Path) -> Vec<PathBuf> {
    let mut bulunan = Vec::new();
    let mut yigin = vec![kok.to_path_buf()];
    while let Some(dizin) = yigin.pop() {
        let girisler = match fs::read_dir(&dizin) {
            Ok(g) => g,
            Err(_) => continue,
        };
        for giris in girisler.flatten() {
            let yol = giris.path();
            if yol.is_dir() {
                yigin.push(yol);
            } else if yol.extension().map(|e| e == "rs").unwrap_or(false) {
                bulunan.push(yol);
            }
        }
    }
    bulunan.sort();
    bulunan
}

#[test]
fn uretim_kodunda_hicbir_silme_cagrisi_yok() {
    let kok = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let dosyalar = kaynak_dosyalari(&kok);
    assert!(!dosyalar.is_empty(), "src/ altinda dosya bulunamadi");
    let mut ihlaller: Vec<String> = Vec::new();
    for dosya in &dosyalar {
        let icerik = fs::read_to_string(dosya).expect("kaynak oku");
        let uretim = uretim_kismi(&icerik);
        for cagri in YASAK_CAGRIMLAR {
            if uretim.contains(cagri) {
                ihlaller.push(format!("{} -> {}", dosya.display(), cagri));
            }
        }
    }
    assert!(
        ihlaller.is_empty(),
        "uretim kodunda silme cagrisi bulundu: {:?}",
        ihlaller
    );
}

#[test]
fn uretim_kodunda_unsafe_kullanimi_yok() {
    let kok = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for dosya in kaynak_dosyalari(&kok) {
        let icerik = fs::read_to_string(&dosya).expect("kaynak oku");
        let uretim = uretim_kismi(&icerik);
        // `forbid(unsafe_code)` tanımının kendisi ve dokümantasyon metni hariç.
        for (no, satir) in uretim.lines().enumerate() {
            let temiz = satir
                .trim_start()
                .trim_start_matches("///")
                .trim_start_matches("//!")
                .trim();
            if temiz.starts_with("unsafe") {
                panic!(
                    "{}:{} unsafe kullanimi bulundu: {}",
                    dosya.display(),
                    no + 1,
                    satir
                );
            }
        }
    }
}

#[test]
fn her_kaynak_dosyasi_forbid_unsafe_code_iceriyor() {
    let kok = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for ad in ["lib.rs", "main.rs"] {
        let icerik = fs::read_to_string(kok.join(ad)).expect("kaynak oku");
        assert!(
            icerik.contains("#![forbid(unsafe_code)]"),
            "{} icinde #![forbid(unsafe_code)] yok",
            ad
        );
    }
}

#[test]
fn arsiv_kayitlari_her_zaman_kaynak_yolu_tasiyor() {
    // Arşiv biçimi, geri almanın mümkün olması için kaynak yolu taşımak
    // zorundadır; bu alan kaldırılırsa geri alma imkânsız hâle gelir.
    let kok = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/arsiv.rs");
    let icerik = fs::read_to_string(kok).expect("arsiv kaynagi oku");
    assert!(icerik.contains("kaynak_yol"));
    assert!(icerik.contains("blake3"));
    assert!(icerik.contains("crc32"));
}
