//! Kullanıcı kuralları: minimum boyut, gömülü/boş dosya, türetilmiş ad,
//! korunacak uzantılar, hariç tutulacak yol desenleri ve akış ayarları.
//!
//! Kurallar düz metin JSON dosyasında tutulur ve Git'e eklenebilir. Yarım
//! okunmuş bir kural dosyasının daha az koruma uygulaması kabul edilmez:
//! bilinmeyen alan görülürse dosya reddedilir ve çağıran taraf hata alır.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hata::{Hata, Sonuc};
use crate::yol;

/// Kural dosyasının öntanılan adı.
pub const VARSAYILAN_KURAL_DOSYASI: &str = "kurallar.json";

/// Varsayılan 2. aşama (önizleme) penceresi: dosyanın baş ve sonundan okunacak bayt.
pub const VARSAYILAN_ONIZLEME_BAYT: usize = 64 * 1024;

/// Varsayılan akış tamponu (raporun 1 MB önerisi).
pub const VARSAYILAN_TAMPON_BAYT: usize = 1024 * 1024;

/// Türetilmiş/geçici dosya adlarını tanıyan uzantılar.
pub const TURETILMIS_UZANTILAR: &[&str] = &[
    "tmp",
    "temp",
    "bak",
    "old",
    "orig",
    "swp",
    "crdownload",
    "part",
    "partial",
    "download",
];

/// Türetilmiş/geçici dosya adlarını tanıyan ad ön ekleri.
pub const TURETILMIS_ONEKLER: &[&str] = &["~$", "~"];

/// Bir taramada uygulanacak tüm kurallar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Kurallar {
    /// Bu boyutun altındaki dosyalar hiç aday olmaz.
    pub minimum_boyut: u64,
    /// 0 baytlık dosyalar atlanır.
    pub bos_dosyalari_atla: bool,
    /// Nokta ile başlayan veya Windows gizli niteliği taşıyan dosyalar atlanır.
    pub gomulu_dosyalari_atla: bool,
    /// `~$`, `~`, `.tmp`, `.bak` gibi geçici/türetilmiş adlar atlanır.
    pub turetilmis_adi_atla: bool,
    /// Bu uzantılara sahip dosyalar arşiv adayı yapılmaz (korunur).
    pub korunacak_uzantilar: BTreeSet<String>,
    /// Bu joker desenlere uyan yollar taramaya alınmaz.
    pub haric_yol_desenleri: Vec<String>,
    /// 2. aşamada dosyanın baş ve sonundan okunacak bayt sayısı.
    pub onizleme_bayt: usize,
    /// Akış karması için sabit tampon boyutu (bayt).
    pub tampon_bayt: usize,
}

impl Default for Kurallar {
    fn default() -> Self {
        Self {
            minimum_boyut: 1,
            bos_dosyalari_atla: true,
            gomulu_dosyalari_atla: false,
            turetilmis_adi_atla: true,
            korunacak_uzantilar: BTreeSet::new(),
            haric_yol_desenleri: vec![
                "*/.git/*".to_string(),
                "*/node_modules/*".to_string(),
                "*/$RECYCLE.BIN/*".to_string(),
                "*/System Volume Information/*".to_string(),
            ],
            onizleme_bayt: VARSAYILAN_ONIZLEME_BAYT,
            tampon_bayt: VARSAYILAN_TAMPON_BAYT,
        }
    }
}

/// Bir dosyanın kurallara uygunluğunun sonucu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KuralKarsiligi {
    /// Dosya taramaya alınır.
    Aday,
    /// Dosya belirli bir gerekçeyle dışlandı.
    Haric {
        /// Kuralın açık metin açıklaması.
        gerekce: String,
    },
}

/// Kurallar ve bunların okunduğu dosya.
#[derive(Debug, Clone)]
pub struct YuklenenKurallar {
    /// Kuralların kendisi.
    pub kurallar: Kurallar,
    /// Kuralların okunduğu dosya.
    pub kaynak: PathBuf,
}

impl Kurallar {
    /// Öntanılan kuralları döndürür.
    pub fn varsayilan() -> Self {
        Self::default()
    }

    /// Kural dosyasını okur, şemayı doğrular ve aritmetiksel tutarlılığı denetler.
    pub fn yukle(dosya: &Path) -> Sonuc<Self> {
        let ham = fs::read_to_string(yol::uzun_yol(dosya))
            .map_err(|kaynak| Hata::io("kural dosyası okuma", dosya, kaynak))?;
        let kurallar: Kurallar =
            serde_json::from_str(&ham).map_err(|hata| Hata::KuralGecersiz {
                dosya: dosya.to_path_buf(),
                ayrinti: hata.to_string(),
            })?;
        kurallar.dogrula(dosya)?;
        Ok(kurallar)
    }

    /// Kuralları okunabilir biçimde JSON olarak diske yazar.
    pub fn kaydet(&self, dosya: &Path) -> Sonuc<()> {
        if let Some(ebeveyn) = dosya.parent() {
            if !ebeveyn.as_os_str().is_empty() {
                fs::create_dir_all(yol::uzun_yol(ebeveyn))
                    .map_err(|kaynak| Hata::io("kural dizini oluşturma", ebeveyn, kaynak))?;
            }
        }
        let mut metin = serde_json::to_string_pretty(self).map_err(|hata| Hata::KuralGecersiz {
            dosya: dosya.to_path_buf(),
            ayrinti: hata.to_string(),
        })?;
        metin.push('\n');
        fs::write(yol::uzun_yol(dosya), metin)
            .map_err(|kaynak| Hata::io("kural dosyası yazma", dosya, kaynak))
    }

    /// Sıfıra eşit sayısal ayarları reddeder.
    pub fn dogrula(&self, dosya: &Path) -> Sonuc<()> {
        if self.onizleme_bayt == 0 {
            return Err(Hata::KuralGecersiz {
                dosya: dosya.to_path_buf(),
                ayrinti: "onizleme_bayt sıfır olamaz".to_string(),
            });
        }
        if self.tampon_bayt == 0 {
            return Err(Hata::KuralGecersiz {
                dosya: dosya.to_path_buf(),
                ayrinti: "tampon_bayt sıfır olamaz".to_string(),
            });
        }
        Ok(())
    }

    /// Bir dosya adının türetilmiş (geçici) bir ad olup olmadığını döndürür.
    pub fn turetilmis_adi_mi(ad: &str) -> bool {
        let kucuk = ad.to_lowercase();
        if TURETILMIS_ONEKLER
            .iter()
            .any(|onek| kucuk.starts_with(onek))
        {
            return true;
        }
        if kucuk.ends_with('~') {
            return true;
        }
        match yol::uzanti(&kucuk) {
            Some(uzanti) => TURETILMIS_UZANTILAR.contains(&uzanti.as_str()),
            None => false,
        }
    }

    /// Uzantısı korunacak listede olan dosyayı belirtir.
    pub fn korunuyor_mu(&self, ad: &str) -> bool {
        match yol::uzanti(ad) {
            Some(uzanti) => self.korunacak_uzantilar.contains(&uzanti),
            None => false,
        }
    }

    /// Verilen yolun hariç tutulacak desenlerden birine uyup uymadığını döndürür.
    pub fn yol_disi_mi(&self, yol: &Path) -> bool {
        let metin = yol::normalleştir(yol);
        self.haric_yol_desenleri
            .iter()
            .any(|desen| yol::desen_eslestir(desen, &metin))
    }

    /// Tüm kuralları tek bir dosya için uygular ve gerekçeli sonucu döndürür.
    pub fn uygula(&self, yol: &Path, boyut: u64, gizli: bool) -> KuralKarsiligi {
        if self.yol_disi_mi(yol) {
            return KuralKarsiligi::Haric {
                gerekce: "hariç yol deseni".to_string(),
            };
        }
        let ad = yol
            .file_name()
            .map(|a| a.to_string_lossy().to_string())
            .unwrap_or_default();
        if self.gomulu_dosyalari_atla && gizli {
            return KuralKarsiligi::Haric {
                gerekce: "gömülü dosya".to_string(),
            };
        }
        if self.bos_dosyalari_atla && boyut == 0 {
            return KuralKarsiligi::Haric {
                gerekce: "boş dosya".to_string(),
            };
        }
        if boyut < self.minimum_boyut {
            return KuralKarsiligi::Haric {
                gerekce: format!("{} bayt < minimum {}", boyut, self.minimum_boyut),
            };
        }
        if self.turetilmis_adi_atla && Kurallar::turetilmis_adi_mi(&ad) {
            return KuralKarsiligi::Haric {
                gerekce: format!("türetilmiş dosya adı: {}", ad),
            };
        }
        KuralKarsiligi::Aday
    }
}

#[cfg(test)]
// Gerekçe: expect/unwrap yalnızca test içinde kullanılır ve testin
// başarısızlık mesajıdır. Üretim kodunda bu lintler açıktır
// (crate seviyesinde clippy::unwrap_used/clippy::expect_used).
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn gecici_dizin(etiket: &str) -> PathBuf {
        let kok = std::env::temp_dir().join(format!("duphunter-kural-{}", etiket));
        let _ = fs::remove_dir_all(&kok);
        fs::create_dir_all(&kok).expect("gecici dizin");
        kok
    }

    #[test]
    fn varsayilan_kurallar_bos_dosyayi_atlar() {
        let k = Kurallar::varsayilan();
        let kars = k.uygula(Path::new("a.txt"), 0, false);
        assert_eq!(
            kars,
            KuralKarsiligi::Haric {
                gerekce: "boş dosya".to_string()
            }
        );
    }

    #[test]
    fn minimum_boyut_kurali_gerekce_yazar() {
        let mut k = Kurallar::varsayilan();
        k.minimum_boyut = 100;
        match k.uygula(Path::new("a.txt"), 10, false) {
            KuralKarsiligi::Haric { gerekce } => {
                assert!(gerekce.contains("minimum"), "{}", gerekce)
            }
            KuralKarsiligi::Aday => panic!("kucuk dosya aday olmamali"),
        }
    }

    #[test]
    fn gomulu_kural_etkinlestirilince_uygulanir() {
        let mut k = Kurallar::varsayilan();
        k.gomulu_dosyalari_atla = true;
        assert!(matches!(
            k.uygula(Path::new(".gizli"), 4096, true),
            KuralKarsiligi::Haric { .. }
        ));
    }

    #[test]
    fn gomulu_kural_kapatilinca_uygulanmaz() {
        let k = Kurallar::varsayilan();
        assert_eq!(
            k.uygula(Path::new(".gizli"), 4096, true),
            KuralKarsiligi::Aday
        );
    }

    #[test]
    fn turetilmis_kural_kapatilinca_dosya_aday_olur() {
        let mut k = Kurallar::varsayilan();
        k.turetilmis_adi_atla = false;
        assert_eq!(
            k.uygula(Path::new("notlar.tmp"), 4096, false),
            KuralKarsiligi::Aday
        );
    }

    #[test]
    fn turetilmis_ad_tespiti_kapsamli() {
        for ad in [
            "~$ belge.docx",
            "notlar.tmp",
            "a.bak",
            "dosya~",
            "video.part",
            "x.partial",
        ] {
            assert!(Kurallar::turetilmis_adi_mi(ad), "{} turetilmis olmali", ad);
        }
        assert!(!Kurallar::turetilmis_adi_mi("rapor.pdf"));
        assert!(!Kurallar::turetilmis_adi_mi("NOTLAR.TXT"));
    }

    #[test]
    fn korunacak_uzanti_listesi_calisir() {
        let mut k = Kurallar::varsayilan();
        k.korunacak_uzantilar.insert("keep".to_string());
        assert!(k.korunuyor_mu("arsiv.keep"));
        assert!(!k.korunuyor_mu("arsiv.txt"));
    }

    #[test]
    fn haric_yol_deseni_git_dizini_dislar() {
        let k = Kurallar::varsayilan();
        assert!(k.yol_disi_mi(Path::new("/ev/repo/.git/objects/aa")));
        assert!(!k.yol_disi_mi(Path::new("/ev/belgeler/a.txt")));
    }

    #[test]
    fn kural_dosyasi_yazilip_okunur() {
        let dizin = gecici_dizin("io");
        let dosya = dizin.join("kurallar.json");
        let k = Kurallar::varsayilan();
        if let Err(hata) = k.kaydet(&dosya) {
            panic!("kaydet: {}", hata);
        }
        match Kurallar::yukle(&dosya) {
            Ok(v) => assert_eq!(v, k),
            Err(hata) => panic!("yukle: {}", hata),
        }
        let _ = fs::remove_dir_all(&dizin);
    }

    #[test]
    fn bilinmeyen_alan_kural_dosyasini_reddeder() {
        let dizin = gecici_dizin("bilinmeyen");
        let dosya = dizin.join("kurallar.json");
        if let Err(hata) = fs::write(&dosya, "{\"minimum_boyut\": 1, \"haric_alan\": 5}") {
            panic!("yaz: {}", hata);
        }
        let sonuc = Kurallar::yukle(&dosya);
        assert!(
            matches!(sonuc, Err(Hata::KuralGecersiz { .. })),
            "bilinmeyen alan reddedilmeli"
        );
        let _ = fs::remove_dir_all(&dizin);
    }

    #[test]
    fn sifir_onizleme_kurali_gecersizdir() {
        let dizin = gecici_dizin("sifir");
        let dosya = dizin.join("kurallar.json");
        if let Err(hata) = fs::write(&dosya, "{\"onizleme_bayt\": 0}") {
            panic!("yaz: {}", hata);
        }
        let sonuc = Kurallar::yukle(&dosya);
        assert!(
            matches!(sonuc, Err(Hata::KuralGecersiz { .. })),
            "{:?}",
            sonuc
        );
        let _ = fs::remove_dir_all(&dizin);
    }
}
