//! Hata tipi ve sonuç takma adı.
//!
//! Bu modül yalnızca hata taşır: hiçbir iş yürütmez, hiçbir dosyaya dokunmaz.
//! Kural dosyası bozuk, dizin okunamıyor veya arşiv doğrulanamıyorsa üretilen
//! hatalar burada toplanır; çağıran taraf `Display` çıktısını kullanıcıya gösterir.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Tüm genel işlemlerin sonuç tipi.
pub type Sonuc<T> = Result<T, Hata>;

/// DupHunter'ın ürettiği hataların tamamı.
///
/// `#[non_exhaustive]` ile işaretlidir: yeni bir hata çeşidi eklendiğinde
/// çağıran tarafın eşleşmesi derleme hatasına dönüşmez.
#[derive(Debug)]
#[non_exhaustive]
pub enum Hata {
    /// Dosya sistemi işlemi başarısız oldu.
    Io {
        /// Yapılmak istenen işlem ("dizin tarama", "dosya kopyala" ...).
        eylem: &'static str,
        /// İşlemin konusu olan yol.
        yol: PathBuf,
        /// Altta yatan işletim sistemi hatası.
        kaynak: io::Error,
    },
    /// Tarama kökü bir dizin değil veya hiç yok.
    TaramaYoluHatali {
        /// Verilen yol.
        yol: PathBuf,
        /// Neden uygun olmadığı.
        ayrinti: String,
    },
    /// Kural dosyası okunamadı, çözümlenemedi veya şemaya uymuyor.
    KuralGecersiz {
        /// Kural dosyasının yolu.
        dosya: PathBuf,
        /// Ayrıntılı açıklama.
        ayrinti: String,
    },
    /// Kurtarma arşivi bozuk: eksik alan, yanlış sürüm veya bozuk JSON.
    ArsivBozuk {
        /// Manifest dosyasının yolu.
        dosya: PathBuf,
        /// Ayrıntılı açıklama.
        ayrinti: String,
    },
    /// Arşivdeki bir dosyanın karması manifest ile uyuşmuyor.
    ArsivDogrulamaBasarisiz {
        /// Doğrulanamayan dosyanın yolu.
        yol: PathBuf,
        /// Manifestte beklenen karma.
        beklenen: String,
        /// Diskte bulunan karma.
        bulunan: String,
    },
    /// Geri alma hedefi zaten var; üzerine yazılmadı.
    HedefVar {
        /// Yazılması istenen yol.
        yol: PathBuf,
    },
    /// Komut satırından gelen eksik veya tutarsız parametre.
    Parametre {
        /// Eksik olan parametrenin adı.
        ad: &'static str,
        /// Açıklama.
        ayrinti: String,
    },
}

impl Hata {
    /// Dosya sistemi hatasını yol ve eylem bağlamıyla sarar.
    pub fn io(eylem: &'static str, yol: &Path, kaynak: io::Error) -> Self {
        Hata::Io {
            eylem,
            yol: yol.to_path_buf(),
            kaynak,
        }
    }
}

impl fmt::Display for Hata {
    fn fmt(&self, bicik: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Hata::Io { eylem, yol, kaynak } => {
                write!(bicik, "{} başarısız ({}): {}", eylem, yol.display(), kaynak)
            }
            Hata::TaramaYoluHatali { yol, ayrinti } => {
                write!(
                    bicik,
                    "tarama yolu geçersiz ({}): {}",
                    yol.display(),
                    ayrinti
                )
            }
            Hata::KuralGecersiz { dosya, ayrinti } => {
                write!(
                    bicik,
                    "kural dosyası geçersiz ({}): {}",
                    dosya.display(),
                    ayrinti
                )
            }
            Hata::ArsivBozuk { dosya, ayrinti } => {
                write!(
                    bicik,
                    "kurtarma arşivi bozuk ({}): {}",
                    dosya.display(),
                    ayrinti
                )
            }
            Hata::ArsivDogrulamaBasarisiz {
                yol,
                beklenen,
                bulunan,
            } => write!(
                bicik,
                "arşiv doğrulaması başarısız ({}): beklenen {}, bulunan {}",
                yol.display(),
                beklenen,
                bulunan
            ),
            Hata::HedefVar { yol } => write!(
                bicik,
                "geri alma hedefi zaten var, üzerine yazılmadı: {}",
                yol.display()
            ),
            Hata::Parametre { ad, ayrinti } => {
                write!(bicik, "parametre hatası ({}): {}", ad, ayrinti)
            }
        }
    }
}

impl std::error::Error for Hata {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Hata::Io { kaynak, .. } => Some(kaynak),
            _ => None,
        }
    }
}

#[cfg(test)]
// Gerekçe: expect/unwrap yalnızca test içinde kullanılır ve testin
// başarısızlık mesajıdır. Üretim kodunda bu lintler açıktır
// (crate seviyesinde clippy::unwrap_used/clippy::expect_used).
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn io_hatasi_yol_ve_eylem_tasir() {
        let kaynak = io::Error::new(io::ErrorKind::NotFound, "yok");
        let h = Hata::io("dizin tarama", Path::new("/tmp/ornek"), kaynak);
        let metin = h.to_string();
        assert!(metin.contains("dizin tarama"), "{}", metin);
        assert!(metin.contains("ornek"), "{}", metin);
    }

    #[test]
    fn arsiv_dogrulama_hatasi_iki_karmayi_yazar() {
        let h = Hata::ArsivDogrulamaBasarisiz {
            yol: PathBuf::from("a.txt"),
            beklenen: "aaaa".into(),
            bulunan: "bbbb".into(),
        };
        let metin = h.to_string();
        assert!(metin.contains("aaaa"), "{}", metin);
        assert!(metin.contains("bbbb"), "{}", metin);
    }

    #[test]
    fn hedeftesti_ustune_yazmadigini_soyler() {
        let h = Hata::HedefVar {
            yol: PathBuf::from("x.txt"),
        };
        assert!(h.to_string().contains("üzerine yazılmadı"));
    }
}
