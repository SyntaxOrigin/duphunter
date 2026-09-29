//! Kurtarma arşivi: karar verilen kopyaları **taşır**, asla silmez.
//!
//! Ürünün güvenlik modeli şudur: hiçbir koşulda dosya silinmez. Bir karar
//! verildiğinde dosya kurtarma dizinine **kopyalanır** ve kaynak yerinde
//! bırakılır; `manifest.json` kaynak yolu, arşiv içi yol, boyut, CRC32,
//! BLAKE3 ve zaman damgasını saklar.
//!
//! Arşiv yazımı iki adımdan geçer: (1) dosyalar kopyalanır ve manifest
//! `manifest.json.partial` adıyla yazılır, (2) arşivdeki her dosya yeniden
//! okunup karması karşılaştırılır. Doğrulama başarısızsa ad değiştirilmez,
//! dolayısıyla yarım kalmış bir arşiv "geçerli" sayılamaz.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::grup::Karar;
use crate::hata::{Hata, Sonuc};
use crate::karma::{karma_hex, tam_karma, AkisKuyrugu, VARSAYILAN_TAMPON_BAYT};
use crate::yol;

/// Arşiv biçiminin sürüm numarası.
pub const ARSIV_SURUMU: u32 = 1;

/// Doğrulanmış manifest dosyasının adı.
pub const MANIFEST_ADI: &str = "manifest.json";

/// Doğrulama tamamlanana kadar kullanılan geçici manifest adı.
pub const KISMI_ADI: &str = "manifest.json.partial";

/// Arşivlenen dosyaların konduğu alt dizin.
pub const DOSYA_KLASORU: &str = "dosyalar";

/// Arşivlenen tek bir dosyanın kaydı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArsivKaydi {
    /// Kopyalandığı özgün yol.
    pub kaynak_yol: PathBuf,
    /// Arşiv dizinine göreli hedef yol.
    pub arsivdeki_yol: PathBuf,
    /// Dosyanın bayt cinsinden boyutu.
    pub boyut: u64,
    /// Hızlı bütünlük sağlaması.
    pub crc32: u32,
    /// Tam içerik BLAKE3 özeti (onaltılık).
    pub blake3: String,
    /// Arşivlenme anının Unix saniyesi.
    pub zaman_sn: u64,
}

/// Arşiv manifestinin tamamı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Biçim sürümü.
    pub surum: u32,
    /// Arşivin oluşturulduğu Unix saniyesi.
    pub olusturma_sn: u64,
    /// Arşivlenen dosya kayıtları.
    pub kayitlar: Vec<ArsivKaydi>,
}

/// Doğrulanmış bir kurtarma arşivi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arsiv {
    /// Arşivin bulunduğu dizin.
    pub dizin: PathBuf,
    /// Okunmuş ve sürümü denetlenmiş manifest.
    pub manifest: Manifest,
}

/// Arşiv doğrulamasının özeti.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DogrulamaOzeti {
    /// Doğrulanan dosya sayısı.
    pub dosya: usize,
    /// Doğrulama sırasında yeniden okunan toplam bayt.
    pub bayt: u64,
}

/// Unix saniyesi cinsinden şu anki zaman.
pub fn simdi_sn() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|s| s.as_secs())
        .unwrap_or(0)
}

/// Arşiv adını taşıyan dosyaları sırayla kopyalar ve manifest yazar.
///
/// `kararlar` içindeki yalnızca `KararTuru::Arsivle` olanlar işlenir. Kaynak
/// dosyalar hiçbir zaman silinmez veya değiştirilmez.
pub fn arsiv_olustur(dizin: &Path, kararlar: &[Karar], kuyruk: &mut AkisKuyrugu) -> Sonuc<Arsiv> {
    fs::create_dir_all(yol::uzun_yol(&dizin.join(DOSYA_KLASORU)))
        .map_err(|hata| Hata::io("kurtarma dizini oluşturma", dizin, hata))?;

    let zaman = simdi_sn();
    let mut kayitlar: Vec<ArsivKaydi> = Vec::new();
    for (sira, karar) in kararlar
        .iter()
        .filter(|k| k.tur == crate::grup::KararTuru::Arsivle)
        .enumerate()
    {
        let ad = karar
            .yol
            .file_name()
            .map(|a| a.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("dosya-{}", sira));
        // Sıra numarası önek olarak kullanılır: farklı dizinlerdeki aynı adlı
        // dosyalar arşivde çakışmaz.
        let goreli = PathBuf::from(DOSYA_KLASORU).join(format!("{:05}_{}", sira, ad));
        let tam_yol = dizin.join(&goreli);
        let kanit = crate::karma::dosya_kopyala_ve_karmala(&karar.yol, &tam_yol, kuyruk)?;
        kayitlar.push(ArsivKaydi {
            kaynak_yol: karar.yol.clone(),
            arsivdeki_yol: goreli,
            boyut: kanit.boyut,
            crc32: kanit.crc32,
            blake3: karma_hex(&kanit.blake3),
            zaman_sn: zaman,
        });
    }

    let manifest = Manifest {
        surum: ARSIV_SURUMU,
        olusturma_sn: zaman,
        kayitlar,
    };
    let kismi = dizin.join(KISMI_ADI);
    let mut metin = serde_json::to_string_pretty(&manifest).map_err(|hata| Hata::ArsivBozuk {
        dosya: kismi.clone(),
        ayrinti: hata.to_string(),
    })?;
    metin.push('\n');
    fs::write(yol::uzun_yol(&kismi), &metin)
        .map_err(|hata| Hata::io("manifest yazma", &kismi, hata))?;

    // Geri okuma testi: arşivdeki dosyalar yeniden okunur ve karmalar karşılaştırılır.
    let arsiv = Arsiv {
        dizin: dizin.to_path_buf(),
        manifest,
    };
    dogrula(&arsiv, kuyruk)?;

    fs::rename(
        yol::uzun_yol(&kismi),
        yol::uzun_yol(&dizin.join(MANIFEST_ADI)),
    )
    .map_err(|hata| Hata::io("manifest doğrulama", dizin, hata))?;
    Ok(arsiv)
}

/// Arşivdeki her dosyayı yeniden okuyup manifestle karşılaştırır.
pub fn dogrula(arsiv: &Arsiv, kuyruk: &mut AkisKuyrugu) -> Sonuc<DogrulamaOzeti> {
    let mut bayt = 0u64;
    for kayit in &arsiv.manifest.kayitlar {
        let yol = arsiv.dizin.join(&kayit.arsivdeki_yol);
        let karma = tam_karma(&yol, kuyruk)?;
        let bulunan = karma_hex(&karma);
        if bulunan != kayit.blake3 {
            return Err(Hata::ArsivDogrulamaBasarisiz {
                yol,
                beklenen: kayit.blake3.clone(),
                bulunan,
            });
        }
        bayt += kayit.boyut;
    }
    Ok(DogrulamaOzeti {
        dosya: arsiv.manifest.kayitlar.len(),
        bayt,
    })
}

/// Manifest dosyasını okur, şemayı ve sürümü doğrular.
pub fn manifest_oku(dosya: &Path) -> Sonuc<Manifest> {
    let ham = fs::read_to_string(yol::uzun_yol(dosya))
        .map_err(|hata| Hata::io("manifest okuma", dosya, hata))?;
    let manifest: Manifest = serde_json::from_str(&ham).map_err(|hata| Hata::ArsivBozuk {
        dosya: dosya.to_path_buf(),
        ayrinti: hata.to_string(),
    })?;
    if manifest.surum != ARSIV_SURUMU {
        return Err(Hata::ArsivBozuk {
            dosya: dosya.to_path_buf(),
            ayrinti: format!(
                "beklenen sürüm {}, bulunan {}",
                ARSIV_SURUMU, manifest.surum
            ),
        });
    }
    Ok(manifest)
}

/// Arşiv dizinindeki doğrulanmış manifesti açar.
pub fn arsiv_ac(dizin: &Path) -> Sonuc<Arsiv> {
    let manifest = manifest_oku(&dizin.join(MANIFEST_ADI))?;
    Ok(Arsiv {
        dizin: dizin.to_path_buf(),
        manifest,
    })
}

/// Bir arşiv dizininde yarım kalmış manifest varsa yolunu döndürür.
///
/// Yarım manifestin varlığı, arşiv yazımının tamamlanmadığı anlamına gelir
/// ve kullanıcıya bildirilmelidir.
pub fn kismi_manifest(dizin: &Path) -> Option<PathBuf> {
    let yol = dizin.join(KISMI_ADI);
    if yol.is_file() {
        Some(yol)
    } else {
        None
    }
}

/// Arşivdeki dosyaları özgün yollarına geri kopyalar ve listeler.
///
/// `hedef_dizin` verilirse kaynaklar özgün dizin yapısı değil, bu dizinin
/// altına yazılır. Yazılacak ad, arşivdeki adın **kendisidir**
/// (`00000_belge.txt` gibi sıra numaralı), çünkü düz bir dizinde yalnızca
/// özgün dosya adını kullanmak farklı klasörlerden gelen aynı adlı
/// dosyaları birbirine yazardı. Mevcut bir hedefin üzerine yazılmaz:
/// `HedefVar` hatası döner.
pub fn geri_al(
    arsiv: &Arsiv,
    hedef_dizin: Option<&Path>,
    kuyruk: &mut AkisKuyrugu,
) -> Sonuc<Vec<PathBuf>> {
    dogrula(arsiv, kuyruk)?;
    let mut yazilan = Vec::new();
    for kayit in &arsiv.manifest.kayitlar {
        let kaynak = arsiv.dizin.join(&kayit.arsivdeki_yol);
        let hedef = match hedef_dizin {
            Some(dizin) => dizin.join(
                kayit
                    .arsivdeki_yol
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("dosya")),
            ),
            None => kayit.kaynak_yol.clone(),
        };
        if hedef.exists() {
            return Err(Hata::HedefVar { yol: hedef });
        }
        if let Some(ebeveyn) = hedef.parent() {
            fs::create_dir_all(yol::uzun_yol(ebeveyn))
                .map_err(|hata| Hata::io("geri alma dizini oluşturma", ebeveyn, hata))?;
        }
        let kanit = crate::karma::dosya_kopyala_ve_karmala(&kaynak, &hedef, kuyruk)?;
        if kanit.boyut != kayit.boyut || karma_hex(&kanit.blake3) != kayit.blake3 {
            return Err(Hata::ArsivDogrulamaBasarisiz {
                yol: hedef,
                beklenen: kayit.blake3.clone(),
                bulunan: karma_hex(&kanit.blake3),
            });
        }
        yazilan.push(hedef);
    }
    Ok(yazilan)
}

/// Varsayılan tampon boyutuyla yeni bir arşiv akış kuyruğu üretir.
pub fn varsayilan_kuyruk() -> Sonuc<AkisKuyrugu> {
    AkisKuyrugu::yeni(VARSAYILAN_TAMPON_BAYT, 2)
}

#[cfg(test)]
// Gerekçe: expect/unwrap yalnızca test içinde kullanılır ve testin
// başarısızlık mesajıdır. Üretim kodunda bu lintler açıktır
// (crate seviyesinde clippy::unwrap_used/clippy::expect_used).
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::grup::{kararlar_uret, KararTuru};
    use crate::kurallar::Kurallar;

    struct Sahne {
        kok: PathBuf,
    }

    impl Sahne {
        fn yeni(etiket: &str) -> Self {
            let kok = std::env::temp_dir().join(format!("duphunter-arsiv-{}", etiket));
            let _ = fs::remove_dir_all(&kok);
            fs::create_dir_all(&kok).expect("gecici dizin");
            Self { kok }
        }
        fn dosya(&self, ad: &str, veri: &[u8]) -> PathBuf {
            let p = self.kok.join("kaynak").join(ad);
            fs::create_dir_all(p.parent().expect("ebeveyn")).expect("dizin");
            fs::write(&p, veri).expect("yaz");
            p
        }
        fn arsiv_dizini(&self) -> PathBuf {
            self.kok.join("kurtarma")
        }
    }

    impl Drop for Sahne {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.kok);
        }
    }

    fn kararlar_icin(gruplar: &[crate::grup::Grup]) -> Vec<Karar> {
        kararlar_uret(gruplar, &Kurallar::varsayilan(), &[])
    }

    #[test]
    fn arsiv_kaynak_dosyayi_silmez_ve_kopya_uretiir() {
        let s = Sahne::yeni("silmez");
        let a = s.dosya("a.txt", b"icerik");
        let b = s.dosya("b.txt", b"icerik");
        let gruplar = crate::grup::asama3_gruplar(&[
            crate::grup::KanitliDosya {
                yol: a.clone(),
                boyut: 6,
                crc32: 1,
                tam_karma: [7u8; 32],
            },
            crate::grup::KanitliDosya {
                yol: b.clone(),
                boyut: 6,
                crc32: 1,
                tam_karma: [7u8; 32],
            },
        ]);
        let mut kuyruk = varsayilan_kuyruk().expect("kuyruk");
        let arsiv = arsiv_olustur(&s.arsiv_dizini(), &kararlar_icin(&gruplar), &mut kuyruk)
            .expect("arsiv olustur");
        assert!(a.is_file(), "kaynak dosya arsivden sonra da yerinde olmali");
        assert!(b.is_file(), "kaynak dosya arsivden sonra da yerinde olmali");
        assert_eq!(arsiv.manifest.kayitlar.len(), 1);
        let kopya = s
            .arsiv_dizini()
            .join(&arsiv.manifest.kayitlar[0].arsivdeki_yol);
        assert_eq!(fs::read(kopya).expect("kopya oku"), b"icerik".to_vec());
    }

    #[test]
    fn arsiv_dogrulama_eksik_dosyada_io_hatasi_dondurur() {
        let s = Sahne::yeni("dogrulama");
        s.dosya("a.txt", b"abc");
        s.dosya("b.txt", b"abc");
        let kayit = ArsivKaydi {
            kaynak_yol: s.kok.join("kaynak/b.txt"),
            arsivdeki_yol: PathBuf::from("dosyalar/00000_b.txt"),
            boyut: 3,
            crc32: 0,
            blake3: karma_hex(&[9u8; 32]),
            zaman_sn: 1,
        };
        let manifest = Manifest {
            surum: ARSIV_SURUMU,
            olusturma_sn: 1,
            kayitlar: vec![kayit],
        };
        let arsiv = Arsiv {
            dizin: s.arsiv_dizini(),
            manifest,
        };
        let mut kuyruk = varsayilan_kuyruk().expect("kuyruk");
        let sonuc = dogrula(&arsiv, &mut kuyruk);
        assert!(matches!(sonuc, Err(Hata::Io { .. })), "{:?}", sonuc);
    }

    #[test]
    fn arsiv_dogrulamasi_bozuk_kopya_yi_yakalar() {
        let s = Sahne::yeni("bozuk");
        let a = s.dosya("a.txt", b"abc");
        let b = s.dosya("b.txt", b"abc");
        let gruplar = crate::grup::asama3_gruplar(&[
            crate::grup::KanitliDosya {
                yol: a,
                boyut: 3,
                crc32: 1,
                tam_karma: [7u8; 32],
            },
            crate::grup::KanitliDosya {
                yol: b,
                boyut: 3,
                crc32: 1,
                tam_karma: [7u8; 32],
            },
        ]);
        let dizin = s.arsiv_dizini();
        let mut kuyruk = varsayilan_kuyruk().expect("kuyruk");
        let arsiv = arsiv_olustur(&dizin, &kararlar_icin(&gruplar), &mut kuyruk).expect("arsiv");
        // Arşivdeki kopyayı boz.
        let kopya = dizin.join(&arsiv.manifest.kayitlar[0].arsivdeki_yol);
        fs::write(&kopya, b"xyz").expect("boz");
        let sonuc = dogrula(&arsiv, &mut kuyruk);
        assert!(
            matches!(sonuc, Err(Hata::ArsivDogrulamaBasarisiz { .. })),
            "bozuk arşiv yakalanmali"
        );
    }

    #[test]
    fn arsiv_geri_alma_bos_hedef_dizine_kopyalar() {
        let s = Sahne::yeni("geri");
        let a = s.dosya("a.txt", b"12345");
        let b = s.dosya("b.txt", b"12345");
        let gruplar = crate::grup::asama3_gruplar(&[
            crate::grup::KanitliDosya {
                yol: a,
                boyut: 5,
                crc32: 1,
                tam_karma: [3u8; 32],
            },
            crate::grup::KanitliDosya {
                yol: b,
                boyut: 5,
                crc32: 1,
                tam_karma: [3u8; 32],
            },
        ]);
        let mut kuyruk = varsayilan_kuyruk().expect("kuyruk");
        let arsiv =
            arsiv_olustur(&s.arsiv_dizini(), &kararlar_icin(&gruplar), &mut kuyruk).expect("arsiv");
        let hedef = s.kok.join("geri-alindi");
        fs::create_dir_all(&hedef).expect("hedef dizini");
        let yazilan = geri_al(&arsiv, Some(&hedef), &mut kuyruk).expect("geri al");
        assert_eq!(yazilan.len(), 1);
        assert_eq!(fs::read(&yazilan[0]).expect("oku"), b"12345".to_vec());
        let ad = yazilan[0]
            .file_name()
            .expect("ad")
            .to_string_lossy()
            .to_string();
        assert!(
            ad.ends_with("b.txt"),
            "korunmayan kopya geri alinmali: {}",
            ad
        );
        assert!(ad.starts_with("0000"), "sira numarasi korunmali: {}", ad);
    }

    #[test]
    fn arsiv_geri_alma_kaynagin_yerinde_durdugu_yola_yazmaz() {
        let s = Sahne::yeni("kaynakyeralti");
        let a = s.dosya("a.txt", b"abc");
        let b = s.dosya("b.txt", b"abc");
        let gruplar = crate::grup::asama3_gruplar(&[
            crate::grup::KanitliDosya {
                yol: a.clone(),
                boyut: 3,
                crc32: 1,
                tam_karma: [3u8; 32],
            },
            crate::grup::KanitliDosya {
                yol: b,
                boyut: 3,
                crc32: 1,
                tam_karma: [3u8; 32],
            },
        ]);
        let mut kuyruk = varsayilan_kuyruk().expect("kuyruk");
        let arsiv =
            arsiv_olustur(&s.arsiv_dizini(), &kararlar_icin(&gruplar), &mut kuyruk).expect("arsiv");
        // Arşiv kopyalamadan sonra kaynak hâlâ durduğu için özgün yola
        // geri alma "hedef var" hatası verir ve üzerine yazmaz.
        let sonuc = geri_al(&arsiv, None, &mut kuyruk);
        assert!(matches!(sonuc, Err(Hata::HedefVar { .. })), "{:?}", sonuc);
        assert_eq!(fs::read(&a).expect("kaynak oku"), b"abc".to_vec());
    }

    #[test]
    fn arsiv_geri_alma_var_olan_hedefin_ustune_yazmaz() {
        let s = Sahne::yeni("cakisma");
        let a = s.dosya("a.txt", b"111");
        let b = s.dosya("b.txt", b"111");
        let gruplar = crate::grup::asama3_gruplar(&[
            crate::grup::KanitliDosya {
                yol: a,
                boyut: 3,
                crc32: 1,
                tam_karma: [3u8; 32],
            },
            crate::grup::KanitliDosya {
                yol: b,
                boyut: 3,
                crc32: 1,
                tam_karma: [3u8; 32],
            },
        ]);
        let mut kuyruk = varsayilan_kuyruk().expect("kuyruk");
        let arsiv =
            arsiv_olustur(&s.arsiv_dizini(), &kararlar_icin(&gruplar), &mut kuyruk).expect("arsiv");
        let hedef = s.kok.join("hedefler");
        fs::create_dir_all(&hedef).expect("hedef dizini");
        let sonuc = geri_al(&arsiv, Some(&hedef), &mut kuyruk);
        // Hedef dizini boş olduğu için ilk geri alma başarılı olmalı,
        // ikincisi HedefVar vermeli.
        if let Err(hata) = sonuc {
            panic!("ilk geri alma basarisiz: {}", hata);
        }
        let sonra = geri_al(&arsiv, Some(&hedef), &mut kuyruk);
        assert!(matches!(sonra, Err(Hata::HedefVar { .. })), "{:?}", sonra);
    }

    #[test]
    fn bozuk_manifest_hata_dondurur() {
        let s = Sahne::yeni("bozukmanifest");
        let dizin = s.arsiv_dizini();
        fs::create_dir_all(&dizin).expect("dizin");
        fs::write(dizin.join(MANIFEST_ADI), "{ bu json degil").expect("yaz");
        let sonuc = manifest_oku(&dizin.join(MANIFEST_ADI));
        assert!(matches!(sonuc, Err(Hata::ArsivBozuk { .. })), "{:?}", sonuc);
    }

    #[test]
    fn yanlis_surumlu_manifest_reddedilir() {
        let s = Sahne::yeni("surum");
        let dizin = s.arsiv_dizini();
        fs::create_dir_all(&dizin).expect("dizin");
        let manifest = Manifest {
            surum: ARSIV_SURUMU + 1,
            olusturma_sn: 0,
            kayitlar: Vec::new(),
        };
        fs::write(
            dizin.join(MANIFEST_ADI),
            serde_json::to_string(&manifest).expect("ser"),
        )
        .expect("yaz");
        let sonuc = manifest_oku(&dizin.join(MANIFEST_ADI));
        assert!(matches!(sonuc, Err(Hata::ArsivBozuk { .. })), "{:?}", sonuc);
    }

    #[test]
    fn olmayan_arsiv_dizini_hata_dondurur() {
        let sonuc = arsiv_ac(Path::new("C:/boyle-arsiv-08-yok"));
        assert!(matches!(sonuc, Err(Hata::Io { .. })), "{:?}", sonuc);
    }

    #[test]
    fn kismi_manifest_varligi_bildirilir() {
        let s = Sahne::yeni("kismi");
        let dizin = s.arsiv_dizini();
        fs::create_dir_all(&dizin).expect("dizin");
        assert!(kismi_manifest(&dizin).is_none());
        fs::write(dizin.join(KISMI_ADI), "{}").expect("yaz");
        assert!(kismi_manifest(&dizin).is_some());
    }

    #[test]
    fn ad_cakismasi_arsivde_ayrilir() {
        let s = Sahne::yeni("adcakismasi");
        let a = s.dosya("bir/dosya.txt", b"aaaa");
        let b = s.dosya("iki/dosya.txt", b"aaaa");
        let gruplar = crate::grup::asama3_gruplar(&[
            crate::grup::KanitliDosya {
                yol: a,
                boyut: 4,
                crc32: 1,
                tam_karma: [4u8; 32],
            },
            crate::grup::KanitliDosya {
                yol: b,
                boyut: 4,
                crc32: 1,
                tam_karma: [4u8; 32],
            },
        ]);
        let dizin = s.arsiv_dizini();
        let mut kuyruk = varsayilan_kuyruk().expect("kuyruk");
        let arsiv = arsiv_olustur(&dizin, &kararlar_icin(&gruplar), &mut kuyruk).expect("arsiv");
        let kayit = &arsiv.manifest.kayitlar[0];
        assert!(kayit.arsivdeki_yol.to_string_lossy().contains("dosya.txt"));
        assert!(dizin.join(&kayit.arsivdeki_yol).is_file());
    }

    #[test]
    fn yalnizca_arsiv_adaylari_islenir() {
        let s = Sahne::yeni("adaylar");
        let a = s.dosya("a.txt", b"aaaa");
        let b = s.dosya("b.txt", b"aaaa");
        let gruplar = crate::grup::asama3_gruplar(&[
            crate::grup::KanitliDosya {
                yol: a,
                boyut: 4,
                crc32: 1,
                tam_karma: [4u8; 32],
            },
            crate::grup::KanitliDosya {
                yol: b,
                boyut: 4,
                crc32: 1,
                tam_karma: [4u8; 32],
            },
        ]);
        let kararlar = kararlar_icin(&gruplar);
        let koru = kararlar.iter().filter(|k| k.tur == KararTuru::Koru).count();
        assert_eq!(koru, 1);
        let mut kuyruk = varsayilan_kuyruk().expect("kuyruk");
        let arsiv = arsiv_olustur(&s.arsiv_dizini(), &kararlar, &mut kuyruk).expect("arsiv");
        assert_eq!(arsiv.manifest.kayitlar.len(), 1);
    }
}
