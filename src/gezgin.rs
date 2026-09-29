//! Kendi özyinelemeli dizin gezgini.
//!
//! `walkdir`/`notify` bağımlılıkları yasak olduğu için gezinme burada
//! yazılmıştır. Yığın (stack) tabanlıdır: çok derin dizin ağacında çağrı
//! yığını taşmaz. Sembolik bağlar ve sabit disk bağları izlenmez; aksi hâlde
//! döngüsel gezinme sayımları bozabilir.
//!
//! Okunamayan bir giriş sessizce atlanmaz: `hatalar` listesine yazılır, çünkü
//! eksik kopyayı "temizlendi" gibi göstermek kullanıcının güvenini kırar.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::hata::{Hata, Sonuc};
use crate::kurallar::{KuralKarsiligi, Kurallar};
use crate::yol;

/// Bir taramada açılacak en fazla alt dizin derinliği.
pub const AZAMI_DERINLIK: usize = 64;

/// Taramayı dışarıdan durdurmak için kullanılan, klonlanabilir bayrak.
#[derive(Debug, Clone, Default)]
pub struct IptalBayragi {
    bayrak: Arc<AtomicBool>,
}

impl IptalBayragi {
    /// Sıfırlanmış yeni bir iptal bayrağı üretir.
    pub fn yeni() -> Self {
        Self::default()
    }

    /// Taramayı durdurma isteği bildirir.
    pub fn iptal_et(&self) {
        self.bayrak.store(true, Ordering::Relaxed);
    }

    /// İptal istenip istenmediğini bildirir.
    pub fn iptal_ildi_mi(&self) -> bool {
        self.bayrak.load(Ordering::Relaxed)
    }
}

/// Windows `FILE_ATTRIBUTE_HIDDEN` biti.
#[cfg(windows)]
const DOSYA_GIZLI_NITELIK: u32 = 0x2;

/// Taramada kurallara uyan bir dosyanın meta verisi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DosyaMeta {
    /// Dosyanın tam yolu.
    pub yol: PathBuf,
    /// Dosyanın bayt cinsinden boyutu.
    pub boyut: u64,
    /// Dosya adının nokta ile başlaması veya gizli dosya niteliği taşıması.
    pub gizli: bool,
    /// Dosya salt okunur.
    pub salt_okunur: bool,
}

/// Bir dosyanın neden aday olmadığını açıklayan kayıt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atlama {
    /// Dışlanan yol.
    pub yol: PathBuf,
    /// Kuralın açık metin gerekçesi.
    pub gerekce: String,
}

/// Taranamayan bir girişin kaydı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HataKaydi {
    /// Sorunlu yol.
    pub yol: PathBuf,
    /// Hata metni.
    pub mesaj: String,
}

/// Bir dizin taramasının tamamı.
#[derive(Debug, Clone, Default)]
pub struct Tarama {
    /// Taranan kök yol.
    pub kok: PathBuf,
    /// Kurallara uyan dosyalar (yol sırasına göre).
    pub dosyalar: Vec<DosyaMeta>,
    /// Kurallar nedeniyle dışlanan dosyalar.
    pub atlananlar: Vec<Atlama>,
    /// Okunamayan dizin/girişler.
    pub hatalar: Vec<HataKaydi>,
    /// Ziyaret edilen dizin sayısı.
    pub dizin_sayisi: usize,
    /// Bağlantı (sembolik bağ, sabit disk bağı) olduğu için izlenmeyen yollar.
    pub baglantilar: Vec<PathBuf>,
    /// Tarama iptal bayrağı yüzünden yarım kaldıysa `true`.
    pub iptal_edildi: bool,
}

impl Tarama {
    /// Taranan dosyaların toplam boyutunu döndürür.
    pub fn toplam_boyut(&self) -> u64 {
        self.dosyalar.iter().map(|d| d.boyut).sum()
    }
}

/// Dizin ağacını kurallara uyan dosya listesi haline getirir.
///
/// `ilerleme` her bulunan dosya için çağrılır; iptal bayrağı verildiğinde
/// tarama kısmi sonuçla döner ve `Tarama::iptal_edildi` işaretlenir.
pub fn tara(
    kok: &Path,
    kurallar: &Kurallar,
    iptal: &IptalBayragi,
    mut ilerleme: impl FnMut(&Path, usize),
) -> Sonuc<Tarama> {
    let kok_meta = fs::metadata(yol::uzun_yol(kok))
        .map_err(|hata| Hata::io("tarama kökü okuma", kok, hata))?;
    if !kok_meta.is_dir() {
        return Err(Hata::TaramaYoluHatali {
            yol: kok.to_path_buf(),
            ayrinti: "verilen yol bir dizin değil".to_string(),
        });
    }

    let mut sonuc = Tarama {
        kok: kok.to_path_buf(),
        ..Tarama::default()
    };
    let mut yigin: VecDeque<(PathBuf, usize)> = VecDeque::new();
    yigin.push_back((kok.to_path_buf(), 0));

    'gezinti: while let Some((dizin, derinlik)) = yigin.pop_front() {
        if iptal.iptal_ildi_mi() {
            sonuc.iptal_edildi = true;
            break;
        }
        if derinlik > AZAMI_DERINLIK {
            sonuc.hatalar.push(HataKaydi {
                yol: dizin.clone(),
                mesaj: format!("azami derinlik ({}) aşıldı, dizin açılmadı", AZAMI_DERINLIK),
            });
            continue;
        }
        sonuc.dizin_sayisi += 1;
        let girdiler = match fs::read_dir(yol::uzun_yol(&dizin)) {
            Ok(g) => g,
            Err(hata) => {
                sonuc.hatalar.push(HataKaydi {
                    yol: dizin.clone(),
                    mesaj: hata.to_string(),
                });
                continue;
            }
        };
        for giris in girdiler {
            // İptal kontrolü dosya düzeyinde de yapılır: kullanıcı uzun bir
            // taramayı tek bir dizinde beklerken durdurabilmelidir.
            if iptal.iptal_ildi_mi() {
                sonuc.iptal_edildi = true;
                break 'gezinti;
            }
            let giris = match giris {
                Ok(g) => g,
                Err(hata) => {
                    sonuc.hatalar.push(HataKaydi {
                        yol: dizin.clone(),
                        mesaj: hata.to_string(),
                    });
                    continue;
                }
            };
            let yol = giris.path();
            // Sembolik bağlar izlenmez: döngüsel gezinme ve sayım bozulması riski.
            if let Ok(baglanti_meta) = fs::symlink_metadata(yol::uzun_yol(&yol)) {
                if baglanti_meta.file_type().is_symlink() {
                    sonuc.baglantilar.push(yol);
                    continue;
                }
            }
            let meta = match fs::metadata(yol::uzun_yol(&yol)) {
                Ok(m) => m,
                Err(hata) => {
                    sonuc.hatalar.push(HataKaydi {
                        yol: yol.clone(),
                        mesaj: hata.to_string(),
                    });
                    continue;
                }
            };
            if meta.is_dir() {
                yigin.push_back((yol, derinlik + 1));
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let ad = giris.file_name().to_string_lossy().to_string();
            let gizli = ad.starts_with('.') || dosya_gizli_mi(&meta);
            match kurallar.uygula(&yol, meta.len(), gizli) {
                KuralKarsiligi::Aday => {
                    ilerleme(&yol, sonuc.dosyalar.len());
                    sonuc.dosyalar.push(DosyaMeta {
                        yol: yol.clone(),
                        boyut: meta.len(),
                        gizli,
                        salt_okunur: meta.permissions().readonly(),
                    });
                }
                KuralKarsiligi::Haric { gerekce } => {
                    sonuc.atlananlar.push(Atlama {
                        yol: yol.clone(),
                        gerekce,
                    });
                }
            }
        }
    }

    sonuc.dosyalar.sort_by(|a, b| a.yol.cmp(&b.yol));
    sonuc.atlananlar.sort_by(|a, b| a.yol.cmp(&b.yol));
    Ok(sonuc)
}

/// Bir dosyanın gizli nitelik taşıyıp taşımadığını işletim sistemine sorar.
///
/// Windows'ta `std::os::windows::fs::MetadataExt::file_attributes` kullanılır;
/// diğer sistemlerde gizlilik yalnızca adın nokta ile başlamasıyla belirlenir
/// ve bu fonksiyon `false` döner.
pub fn dosya_gizli_mi(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & DOSYA_GIZLI_NITELIK != 0
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        false
    }
}

#[cfg(test)]
// Gerekçe: expect/unwrap yalnızca test içinde kullanılır ve testin
// başarısızlık mesajıdır. Üretim kodunda bu lintler açıktır
// (crate seviyesinde clippy::unwrap_used/clippy::expect_used).
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::kurallar::Kurallar;

    struct GeciciDizin {
        yol: PathBuf,
    }

    impl GeciciDizin {
        fn yeni(etiket: &str) -> Self {
            let kok = std::env::temp_dir().join(format!("duphunter-gezgin-{}", etiket));
            let _ = fs::remove_dir_all(&kok);
            fs::create_dir_all(&kok).expect("gecici dizin");
            Self { yol: kok }
        }
        fn yol(&self) -> &Path {
            &self.yol
        }
        fn dosya(&self, ad: &str, veri: &[u8]) -> PathBuf {
            let p = self.yol.join(ad);
            if let Some(ebeveyn) = p.parent() {
                fs::create_dir_all(ebeveyn).expect("alt dizin");
            }
            fs::write(&p, veri).expect("dosya yaz");
            p
        }
    }

    impl Drop for GeciciDizin {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.yol);
        }
    }

    #[test]
    fn tara_dosyalari_yol_sirasina_gore_dondurur() {
        let g = GeciciDizin::yeni("sira");
        g.dosya("b.txt", b"bb");
        g.dosya("a.txt", b"a");
        let t = tara(
            g.yol(),
            &Kurallar::varsayilan(),
            &IptalBayragi::yeni(),
            |_, _| {},
        )
        .expect("tara");
        assert_eq!(t.dosyalar.len(), 2);
        let adlar: Vec<String> = t
            .dosyalar
            .iter()
            .map(|d| d.yol.file_name().expect("ad").to_string_lossy().to_string())
            .collect();
        assert_eq!(adlar, vec!["a.txt".to_string(), "b.txt".to_string()]);
    }

    #[test]
    fn tara_bos_dosyayi_kuralda_atlar() {
        let g = GeciciDizin::yeni("bos");
        g.dosya("bos.txt", b"");
        g.dosya("dolu.txt", b"x");
        let t = tara(
            g.yol(),
            &Kurallar::varsayilan(),
            &IptalBayragi::yeni(),
            |_, _| {},
        )
        .expect("tara");
        assert_eq!(t.dosyalar.len(), 1);
        assert_eq!(t.atlananlar.len(), 1);
        assert_eq!(t.atlananlar[0].gerekce, "boş dosya");
    }

    #[test]
    fn tara_alt_dizinlere_de_girer() {
        let g = GeciciDizin::yeni("alt");
        g.dosya("bir/iki/uc.txt", b"icerik");
        let t = tara(
            g.yol(),
            &Kurallar::varsayilan(),
            &IptalBayragi::yeni(),
            |_, _| {},
        )
        .expect("tara");
        assert_eq!(t.dosyalar.len(), 1);
        assert!(
            t.dizin_sayisi >= 3,
            "alt dizinler sayilmali: {}",
            t.dizin_sayisi
        );
    }

    #[test]
    fn tara_hicbir_ayar_degisikligi_yapmaz() {
        let g = GeciciDizin::yeni("dokunma");
        g.dosya("a.txt", b"12345");
        let once = fs::metadata(g.yol().join("a.txt")).expect("meta").len();
        let _ = tara(
            g.yol(),
            &Kurallar::varsayilan(),
            &IptalBayragi::yeni(),
            |_, _| {},
        );
        let sonra = fs::metadata(g.yol().join("a.txt")).expect("meta").len();
        assert_eq!(once, sonra);
    }

    #[test]
    fn tara_olmayan_yol_hatasi_dondurur() {
        let sonuc = tara(
            Path::new("C:/boyle-bir-dizin-yok-08"),
            &Kurallar::varsayilan(),
            &IptalBayragi::yeni(),
            |_, _| {},
        );
        assert!(matches!(sonuc, Err(Hata::Io { .. })), "{:?}", sonuc);
    }

    #[test]
    fn tara_dosya_kokunu_kabul_etmez() {
        let g = GeciciDizin::yeni("kokdosya");
        let dosya = g.dosya("a.txt", b"x");
        let sonuc = tara(
            &dosya,
            &Kurallar::varsayilan(),
            &IptalBayragi::yeni(),
            |_, _| {},
        );
        assert!(
            matches!(sonuc, Err(Hata::TaramaYoluHatali { .. })),
            "dizin olmayan kok reddedilmeli"
        );
    }

    #[test]
    fn tara_iptal_bayragi_ile_yarim_kalir() {
        let g = GeciciDizin::yeni("iptal");
        for i in 0..6 {
            g.dosya(&format!("dosya{}.txt", i), b"veri");
        }
        let iptal = IptalBayragi::yeni();
        let iptal2 = iptal.clone();
        let t = tara(g.yol(), &Kurallar::varsayilan(), &iptal, |_, sayi| {
            if sayi == 1 {
                iptal2.iptal_et();
            }
        })
        .expect("tara");
        assert!(t.iptal_edildi, "iptal istegi islenmeli");
        assert!(t.dosyalar.len() < 6, "yarim tarama bekleniyor");
    }

    #[test]
    fn iptal_bayragi_klonu_paylasilir() {
        let a = IptalBayragi::yeni();
        let b = a.clone();
        assert!(!a.iptal_ildi_mi());
        b.iptal_et();
        assert!(a.iptal_ildi_mi());
    }
}
