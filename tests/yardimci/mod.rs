//! Entegrasyon testlerinin ortak yardımcıları.
//!
//! `tempfile` crate'i bağımlılık politikasında yasaktır (WORKER_CONTRACT.md
//! § 5.3); bu yüzden geçici dizin üretimi kendi kodumuzla yazılır.

use std::fs;
use std::path::{Path, PathBuf};

/// Test içinde geçici dosya/dizin üreten, `Drop` ile temizleyen kapsayıcı.
///
/// Benzersizlik `std::process::id()` ve etiketten gelir; rastgelelik crate'i
/// kullanılmaz. Aynı etiketle ikinci bir dizin açılırsa eski içerik önce
/// silinir, böylece test tekrarlandığında eski kalıntı bulunmaz.
pub struct GeciciDizin {
    yol: PathBuf,
}

impl GeciciDizin {
    /// `std::env::temp_dir()` altında, etiketten türetilmiş dizin oluşturur.
    pub fn yeni(etiket: &str) -> std::io::Result<Self> {
        let kok =
            std::env::temp_dir().join(format!("duphunter-it-{}-{}", etiket, std::process::id()));
        let _ = fs::remove_dir_all(&kok);
        fs::create_dir_all(&kok)?;
        Ok(Self { yol: kok })
    }

    /// Dizinin tam yolunu verir.
    pub fn yol(&self) -> &Path {
        &self.yol
    }

    /// İçeriği verilen dosyayı oluşturur ve yolunu döndürür.
    pub fn dosya(&self, ad: &str, veri: &[u8]) -> PathBuf {
        let p = self.yol.join(ad);
        if let Some(ebeveyn) = p.parent() {
            fs::create_dir_all(ebeveyn).expect("ebeveyn dizini olustur");
        }
        fs::write(&p, veri).expect("gecici dosya yaz");
        p
    }
}

impl Drop for GeciciDizin {
    fn drop(&mut self) {
        // Temizlik başarısız olsa da testi düşürmemeli; hata `Drop` içinden
        // döndürülemez, bu yüzden bilinçli olarak yutulur.
        let _ = fs::remove_dir_all(&self.yol);
    }
}

/// Deterministik, tekrarlanabilir sahte veri üretir (kendi PRNG'miz yok:
/// doğrusal sayaçtan türetilir).
pub fn sahte_veri(bayt: usize, tohum: u8) -> Vec<u8> {
    (0..bayt)
        .map(|i| ((i as u32 * 31 + u32::from(tohum) * 7) % 251) as u8)
        .collect()
}
