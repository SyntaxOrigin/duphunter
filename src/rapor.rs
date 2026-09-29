//! Kanıt raporu: JSON (makine okunur) ve Markdown (insan okur) çıktısı.
//!
//! Rapor, "neden bu aday?" sorusunu yanıtlamak zorundadır: her grupta
//! ortak BLAKE3 kanıtı, her dosyada CRC32, uygulanan kurallar, atlanan
//! dosyaların gerekçesi ve okunamayan girişlerin listesi bulunur.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::arsiv::simdi_sn;
use crate::gezgin::Tarama;
use crate::grup::{Karar, KararTuru};
use crate::hata::{Hata, Sonuc};
use crate::karma::karma_hex;
use crate::kurallar::Kurallar;
use crate::motor::MotorSonucu;
use crate::yol;

/// Raporun şema sürümü.
pub const RAPOR_SEMASI: u32 = 1;

/// Aracın sürüm dizesi; raporun hangi sürümle üretildiğini belirtir.
pub const ARAC_SURUMU: &str = concat!("duphunter ", env!("CARGO_PKG_VERSION"));

/// Tarama özeti.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ozet {
    /// Taramaya alınan dosya sayısı.
    pub taranan_dosya: usize,
    /// Kurallar nedeniyle dışlanan dosya sayısı.
    pub atlanan_dosya: usize,
    /// Okunamayan giriş sayısı.
    pub okunamayan: usize,
    /// Bulunan kopya grubu sayısı.
    pub grup: usize,
    /// Grup içindeki dosya sayısının toplamı.
    pub kopya_dosya: usize,
    /// Geri kazanılabilir toplam bayt.
    pub kazanc_bayt: u64,
    /// Arşivlenmesi önerilen dosya sayısı.
    pub arsiv_adayi: usize,
    /// Korunması önerilen dosya sayısı.
    pub korunan: usize,
}

/// Aşama bazlı okuma sayacının rapordaki hâli.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SayacRaporu {
    /// 1. aşamaya giren dosya sayısı.
    pub asama1_dosya: usize,
    /// 2. aşamada ön izleme üretilen dosya sayısı.
    pub asama2_dosya: usize,
    /// 3. aşamada tam karma üretilen dosya sayısı.
    pub asama3_dosya: usize,
    /// 2. aşamada okunan bayt.
    pub asama2_bayt: u64,
    /// 3. aşamada okunan bayt.
    pub asama3_bayt: u64,
    /// Taranan dosyaların toplam boyutu.
    pub toplam_boyut: u64,
}

/// Bir grup üyesinin rapordaki hâli.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UyeRaporu {
    /// Dosya yolu.
    pub yol: PathBuf,
    /// Dosya boyutu.
    pub boyut: u64,
    /// 2. aşama CRC32 sağlaması.
    pub crc32: String,
    /// Tam içerik BLAKE3 kanıtı (onaltılık).
    pub blake3: String,
    /// Bu dosya için verilen karar ("koru", "arsivle" veya "asla").
    pub karar: String,
}

/// Bir kopya grubunun rapordaki hâli.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrupRaporu {
    /// Grup sıra numarası.
    pub kimlik: usize,
    /// Ortak boyut.
    pub boyut: u64,
    /// Ortak BLAKE3 kanıtı.
    pub blake3: String,
    /// Üye sayısı.
    pub uye_sayisi: usize,
    /// Geri kazanılacak bayt.
    pub kazanc_bayt: u64,
    /// Grup üyeleri.
    pub uyeler: Vec<UyeRaporu>,
}

/// Dışlanan dosya kaydının rapordaki hâli.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtlamaRaporu {
    /// Dosya yolu.
    pub yol: PathBuf,
    /// Kural gerekçesi.
    pub gerekce: String,
}

/// Okunamayan giriş kaydının rapordaki hâli.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HataRaporu {
    /// Sorunlu yol.
    pub yol: PathBuf,
    /// Hata metni.
    pub mesaj: String,
}

/// Tam kanıt raporu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaramaRaporu {
    /// Şema sürümü.
    pub sema: u32,
    /// Raporu üreten araç ve sürümü.
    pub arac: String,
    /// Taranan kök dizin.
    pub kok: PathBuf,
    /// Raporun üretildiği Unix saniyesi.
    pub zaman_sn: u64,
    /// Tarama iptal edildiyse `true`.
    pub iptal_edildi: bool,
    /// Özet.
    pub ozet: Ozet,
    /// Uygulanan kurallar.
    pub kurallar: Kurallar,
    /// Okuma sayacı.
    pub sayac: SayacRaporu,
    /// Kopya grupları.
    pub gruplar: Vec<GrupRaporu>,
    /// Dışlanan dosyalar.
    pub atlananlar: Vec<AtlamaRaporu>,
    /// Okunamayan girişler.
    pub hatalar: Vec<HataRaporu>,
}

/// Bayt değerini insan okunur biçime çevirir (B/KiB/MiB/GiB).
pub fn bayt_goster(bayt: u64) -> String {
    if bayt < 1024 {
        return format!("{} B", bayt);
    }
    const BIRIM: [(&str, u64); 3] = [
        ("GiB", 1024 * 1024 * 1024),
        ("MiB", 1024 * 1024),
        ("KiB", 1024),
    ];
    for (ad, carpan) in BIRIM {
        if bayt >= carpan {
            return format!("{:.2} {}", bayt as f64 / carpan as f64, ad);
        }
    }
    format!("{} B", bayt)
}

/// Karar türünü rapor metnindeki adına çevirir.
pub fn karar_adi(tur: KararTuru) -> &'static str {
    match tur {
        KararTuru::Koru => "koru",
        KararTuru::Arsivle => "arsivle",
        KararTuru::Asla => "asla",
    }
}

/// Tarama sonucundan kanıt raporunu üretir.
pub fn rapor_uret(
    tarama: &Tarama,
    motor: &MotorSonucu,
    kurallar: &Kurallar,
    kararlar: &[Karar],
) -> TaramaRaporu {
    let kazanc: u64 = motor.gruplar.iter().map(|g| g.kazanc_bayt()).sum();
    let kopya_dosya: usize = motor.gruplar.iter().map(|g| g.uyeler.len()).sum();
    let ozet = Ozet {
        taranan_dosya: tarama.dosyalar.len(),
        atlanan_dosya: tarama.atlananlar.len(),
        okunamayan: tarama.hatalar.len(),
        grup: motor.gruplar.len(),
        kopya_dosya,
        kazanc_bayt: kazanc,
        arsiv_adayi: kararlar
            .iter()
            .filter(|k| k.tur == KararTuru::Arsivle)
            .count(),
        korunan: kararlar.iter().filter(|k| k.tur == KararTuru::Koru).count(),
    };
    let sayac = SayacRaporu {
        asama1_dosya: motor.sayac.asama1_dosya,
        asama2_dosya: motor.sayac.asama2_dosya,
        asama3_dosya: motor.sayac.asama3_dosya,
        asama2_bayt: motor.sayac.asama2_bayt,
        asama3_bayt: motor.sayac.asama3_bayt,
        toplam_boyut: tarama.toplam_boyut(),
    };
    let gruplar = motor
        .gruplar
        .iter()
        .map(|grup| GrupRaporu {
            kimlik: grup.kimlik,
            boyut: grup.boyut,
            blake3: karma_hex(&grup.uyeler[0].tam_karma),
            uye_sayisi: grup.uyeler.len(),
            kazanc_bayt: grup.kazanc_bayt(),
            uyeler: grup
                .uyeler
                .iter()
                .map(|uye| UyeRaporu {
                    yol: uye.yol.clone(),
                    boyut: uye.boyut,
                    crc32: format!("{:08x}", uye.crc32),
                    blake3: karma_hex(&uye.tam_karma),
                    karar: kararlar
                        .iter()
                        .find(|k| k.yol == uye.yol)
                        .map(|k| karar_adi(k.tur).to_string())
                        .unwrap_or_else(|| "belirsiz".to_string()),
                })
                .collect(),
        })
        .collect();

    TaramaRaporu {
        sema: RAPOR_SEMASI,
        arac: ARAC_SURUMU.to_string(),
        kok: tarama.kok.clone(),
        zaman_sn: simdi_sn(),
        iptal_edildi: motor.iptal_edildi || tarama.iptal_edildi,
        ozet,
        kurallar: kurallar.clone(),
        sayac,
        gruplar,
        atlananlar: tarama
            .atlananlar
            .iter()
            .map(|a| AtlamaRaporu {
                yol: a.yol.clone(),
                gerekce: a.gerekce.clone(),
            })
            .collect(),
        hatalar: tarama
            .hatalar
            .iter()
            .map(|h| HataRaporu {
                yol: h.yol.clone(),
                mesaj: h.mesaj.clone(),
            })
            .collect(),
    }
}

/// Raporu JSON olarak diske yazar.
pub fn json_yaz(rapor: &TaramaRaporu, dosya: &Path) -> Sonuc<()> {
    if let Some(ebeveyn) = dosya.parent() {
        if !ebeveyn.as_os_str().is_empty() {
            fs::create_dir_all(yol::uzun_yol(ebeveyn))
                .map_err(|hata| Hata::io("rapor dizini oluşturma", ebeveyn, hata))?;
        }
    }
    let mut metin = serde_json::to_string_pretty(rapor).map_err(|hata| Hata::Io {
        eylem: "rapor serileştirme",
        yol: dosya.to_path_buf(),
        kaynak: std::io::Error::new(std::io::ErrorKind::InvalidData, hata),
    })?;
    metin.push('\n');
    fs::write(yol::uzun_yol(dosya), metin).map_err(|hata| Hata::io("rapor yazma", dosya, hata))
}

/// Raporu okunanır Markdown metnine çevirir.
pub fn markdown_uret(rapor: &TaramaRaporu) -> String {
    let mut cikti = String::new();
    cikti.push_str("# DupHunter kanıt raporu\n\n");
    cikti.push_str(&format!("- Araç: {}\n", rapor.arac));
    cikti.push_str(&format!("- Şema sürümü: {}\n", rapor.sema));
    cikti.push_str(&format!("- Taranan kök: `{}`\n", rapor.kok.display()));
    cikti.push_str(&format!("- Zaman: {}\n", rapor.zaman_sn));
    cikti.push_str(&format!(
        "- Tarama durumu: {}\n\n",
        if rapor.iptal_edildi {
            "İPTAL EDİLDİ (sonuç eksik)"
        } else {
            "tamamlandı"
        }
    ));

    cikti.push_str("## Özet\n\n");
    cikti.push_str(&format!(
        "| Ölçüt | Değer |\n|---|---|\n\
         | Taranan dosya | {} |\n\
         | Dışlanan dosya | {} |\n\
         | Okunamayan giriş | {} |\n\
         | Kopya grubu | {} |\n\
         | Kopyadaki dosya | {} |\n\
         | Geri kazanılabilir | {} ({} bayt) |\n\
         | Arşiv adayı | {} |\n\
         | Korunacak | {} |\n\n",
        rapor.ozet.taranan_dosya,
        rapor.ozet.atlanan_dosya,
        rapor.ozet.okunamayan,
        rapor.ozet.grup,
        rapor.ozet.kopya_dosya,
        bayt_goster(rapor.ozet.kazanc_bayt),
        rapor.ozet.kazanc_bayt,
        rapor.ozet.arsiv_adayi,
        rapor.ozet.korunan
    ));

    cikti.push_str("## Kademeli aday üretimi\n\n");
    cikti.push_str(&format!(
        "| Aşama | Dosya | Okunan bayt |\n|---|---|---|\n\
         | 1. boyut gruplama | {} | 0 |\n\
         | 2. ön eleme (CRC32 + baş/son BLAKE3) | {} | {} |\n\
         | 3. tam akış BLAKE3 | {} | {} |\n\
         | **toplam** | | **{}** (taranan toplam: {}) |\n\n",
        rapor.sayac.asama1_dosya,
        rapor.sayac.asama2_dosya,
        rapor.sayac.asama2_bayt,
        rapor.sayac.asama3_dosya,
        rapor.sayac.asama3_bayt,
        rapor.sayac.asama2_bayt + rapor.sayac.asama3_bayt,
        rapor.sayac.toplam_boyut
    ));

    cikti.push_str("## Uygulanan kurallar\n\n");
    cikti.push_str(&format!(
        "- minimum_boyut: {}\n\
         - bos_dosyalari_atla: {}\n\
         - gomulu_dosyalari_atla: {}\n\
         - turetilmis_adi_atla: {}\n\
         - korunacak_uzantilar: {}\n\
         - onizleme_bayt: {}\n\
         - tampon_bayt: {}\n\
         - haric_yol_desenleri: {}\n\n",
        rapor.kurallar.minimum_boyut,
        rapor.kurallar.bos_dosyalari_atla,
        rapor.kurallar.gomulu_dosyalari_atla,
        rapor.kurallar.turetilmis_adi_atla,
        if rapor.kurallar.korunacak_uzantilar.is_empty() {
            "(yok)".to_string()
        } else {
            rapor
                .kurallar
                .korunacak_uzantilar
                .iter()
                .cloned()
                .collect::<Vec<String>>()
                .join(", ")
        },
        rapor.kurallar.onizleme_bayt,
        rapor.kurallar.tampon_bayt,
        rapor.kurallar.haric_yol_desenleri.join(", ")
    ));

    cikti.push_str("## Kopya grupları\n\n");
    if rapor.gruplar.is_empty() {
        cikti.push_str("Birebir kopya bulunmadı.\n\n");
    } else {
        for grup in &rapor.gruplar {
            cikti.push_str(&format!(
                "### Grup {} — {} bayt, {} kopya, kazanç {} ({} bayt)\n\n",
                grup.kimlik,
                grup.boyut,
                grup.uye_sayisi,
                bayt_goster(grup.kazanc_bayt),
                grup.kazanc_bayt
            ));
            cikti.push_str(&format!("Kanıt (BLAKE3): `{}`\n\n", grup.blake3));
            cikti.push_str("| Karar | Bayt | CRC32 | Yol |\n|---|---|---|---|\n");
            for uye in &grup.uyeler {
                cikti.push_str(&format!(
                    "| {} | {} | `{}` | `{}` |\n",
                    uye.karar,
                    uye.boyut,
                    uye.crc32,
                    uye.yol.display()
                ));
            }
            cikti.push('\n');
        }
    }

    if !rapor.atlananlar.is_empty() {
        cikti.push_str("## Dışlanan dosyalar\n\n| Yol | Gerekçe |\n|---|---|\n");
        for atla in &rapor.atlananlar {
            cikti.push_str(&format!(
                "| `{}` | {} |\n",
                atla.yol.display(),
                atla.gerekce
            ));
        }
        cikti.push('\n');
    }

    if !rapor.hatalar.is_empty() {
        cikti.push_str("## Okunamayan girişler\n\n| Yol | Hata |\n|---|---|\n");
        for hata in &rapor.hatalar {
            cikti.push_str(&format!("| `{}` | {} |\n", hata.yol.display(), hata.mesaj));
        }
        cikti.push('\n');
    }

    cikti.push_str(
        "> Bu araç hiçbir dosyayı silmez. Arşivlenen kopyalar kurtarma dizininde \
         tutulur ve `duphunter restore` ile özgün yollarına geri alınabilir.\n",
    );
    cikti
}

/// Raporu Markdown olarak diske yazar.
pub fn markdown_yaz(rapor: &TaramaRaporu, dosya: &Path) -> Sonuc<()> {
    if let Some(ebeveyn) = dosya.parent() {
        if !ebeveyn.as_os_str().is_empty() {
            fs::create_dir_all(yol::uzun_yol(ebeveyn))
                .map_err(|hata| Hata::io("rapor dizini oluşturma", ebeveyn, hata))?;
        }
    }
    fs::write(yol::uzun_yol(dosya), markdown_uret(rapor))
        .map_err(|hata| Hata::io("rapor yazma", dosya, hata))
}

#[cfg(test)]
// Gerekçe: expect/unwrap yalnızca test içinde kullanılır ve testin
// başarısızlık mesajıdır. Üretim kodunda bu lintler açıktır
// (crate seviyesinde clippy::unwrap_used/clippy::expect_used).
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::gezgin::{DosyaMeta, IptalBayragi};
    use crate::grup::asama3_gruplar;
    use crate::grup::KanitliDosya;
    use crate::motor::{gruplari_uret, OkumaSayaci};
    use std::fs;

    fn ornek_tarama() -> (Tarama, MotorSonucu) {
        let tarama = Tarama {
            kok: PathBuf::from("/ornek"),
            dosyalar: vec![
                DosyaMeta {
                    yol: PathBuf::from("/ornek/a.txt"),
                    boyut: 100,
                    gizli: false,
                    salt_okunur: false,
                },
                DosyaMeta {
                    yol: PathBuf::from("/ornek/b.txt"),
                    boyut: 100,
                    gizli: false,
                    salt_okunur: false,
                },
            ],
            atlananlar: Vec::new(),
            hatalar: vec![crate::gezgin::HataKaydi {
                yol: PathBuf::from("/ornek/okunamayan"),
                mesaj: "izin yok".to_string(),
            }],
            dizin_sayisi: 1,
            baglantilar: Vec::new(),
            iptal_edildi: false,
        };
        let motor = MotorSonucu {
            gruplar: asama3_gruplar(&[
                KanitliDosya {
                    yol: PathBuf::from("/ornek/a.txt"),
                    boyut: 100,
                    crc32: 0xdead_beef,
                    tam_karma: [3u8; 32],
                },
                KanitliDosya {
                    yol: PathBuf::from("/ornek/b.txt"),
                    boyut: 100,
                    crc32: 0xdead_beef,
                    tam_karma: [3u8; 32],
                },
            ]),
            sayac: OkumaSayaci {
                asama1_dosya: 2,
                asama2_dosya: 2,
                asama3_dosya: 2,
                asama2_bayt: 200,
                asama3_bayt: 200,
            },
            iptal_edildi: false,
        };
        (tarama, motor)
    }

    #[test]
    fn bayt_goster_birimleri_secer() {
        assert_eq!(bayt_goster(512), "512 B");
        assert_eq!(bayt_goster(2048), "2.00 KiB");
        assert_eq!(bayt_goster(5 * 1024 * 1024), "5.00 MiB");
        assert_eq!(bayt_goster(3 * 1024 * 1024 * 1024), "3.00 GiB");
    }

    #[test]
    fn karar_adi_turleri_yazar() {
        assert_eq!(karar_adi(KararTuru::Koru), "koru");
        assert_eq!(karar_adi(KararTuru::Arsivle), "arsivle");
        assert_eq!(karar_adi(KararTuru::Asla), "asla");
    }

    #[test]
    fn rapor_uret_ozeti_dogru_hesaplar() {
        let (tarama, motor) = ornek_tarama();
        let kararlar = crate::grup::kararlar_uret(&motor.gruplar, &Kurallar::varsayilan(), &[]);
        let rapor = rapor_olet(&tarama, &motor, kararlar);
        assert_eq!(rapor.ozet.taranan_dosya, 2);
        assert_eq!(rapor.ozet.grup, 1);
        assert_eq!(rapor.ozet.kazanc_bayt, 100);
        assert_eq!(rapor.ozet.arsiv_adayi, 1);
        assert_eq!(rapor.ozet.korunan, 1);
        assert_eq!(rapor.ozet.okunamayan, 1);
    }

    fn rapor_olet(tarama: &Tarama, motor: &MotorSonucu, kararlar: Vec<Karar>) -> TaramaRaporu {
        rapor_uret(tarama, motor, &Kurallar::varsayilan(), &kararlar)
    }

    #[test]
    fn rapor_json_semasi_geri_okunur() {
        let (tarama, motor) = ornek_tarama();
        let kararlar = crate::grup::kararlar_uret(&motor.gruplar, &Kurallar::varsayilan(), &[]);
        let rapor = rapor_olet(&tarama, &motor, kararlar);
        let metin = serde_json::to_string_pretty(&rapor).expect("ser");
        for anahtar in [
            "\"sema\"",
            "\"arac\"",
            "\"kok\"",
            "\"ozet\"",
            "\"kurallar\"",
            "\"sayac\"",
            "\"gruplar\"",
        ] {
            assert!(metin.contains(anahtar), "eksik alan: {}", anahtar);
        }
        let geri: TaramaRaporu = serde_json::from_str(&metin).expect("ayristir");
        assert_eq!(geri, rapor);
    }

    #[test]
    fn rapor_bilinmeyen_alani_reddeder() {
        let mut deger = serde_json::json!({
            "sema": 1, "arac": "x", "kok": "/a", "zaman_sn": 0, "iptal_edildi": false,
            "ozet": {"taranan_dosya":0,"atlanan_dosya":0,"okunamayan":0,"grup":0,
                     "kopya_dosya":0,"kazanc_bayt":0,"arsiv_adayi":0,"korunan":0},
            "kurallar": Kurallar::varsayilan(),
            "sayac": {"asama1_dosya":0,"asama2_dosya":0,"asama3_dosya":0,
                      "asama2_bayt":0,"asama3_bayt":0,"toplam_boyut":0},
            "gruplar": [], "atlananlar": [], "hatalar": []
        });
        deger["bilinmeyen"] = serde_json::json!(1);
        let sonuc: Result<TaramaRaporu, _> = serde_json::from_value(deger);
        assert!(sonuc.is_err(), "bilinmeyen alan reddedilmeli");
    }

    #[test]
    fn markdown_rapor_kaniti_ve_kazanci_gosterir() {
        let (tarama, motor) = ornek_tarama();
        let kararlar = crate::grup::kararlar_uret(&motor.gruplar, &Kurallar::varsayilan(), &[]);
        let rapor = rapor_olet(&tarama, &motor, kararlar);
        let md = markdown_uret(&rapor);
        assert!(md.contains("# DupHunter kanıt raporu"));
        assert!(md.contains("0303030303030303030303030303030303030303030303030303030303030303"));
        assert!(md.contains("deadbeef"));
        assert!(md.contains("hiçbir dosyayı silmez"));
    }

    #[test]
    fn markdown_rapor_dosyaya_yazilir_ve_okunur() {
        let kok = std::env::temp_dir().join("duphunter-rapor-testi");
        let _ = fs::remove_dir_all(&kok);
        fs::create_dir_all(&kok).expect("dizin");
        let (tarama, motor) = ornek_tarama();
        let kararlar = crate::grup::kararlar_uret(&motor.gruplar, &Kurallar::varsayilan(), &[]);
        let rapor = rapor_olet(&tarama, &motor, kararlar);
        let md = kok.join("rapor.md");
        let js = kok.join("rapor.json");
        if let Err(hata) = markdown_yaz(&rapor, &md) {
            panic!("markdown yaz: {}", hata);
        }
        if let Err(hata) = json_yaz(&rapor, &js) {
            panic!("json yaz: {}", hata);
        }
        assert!(fs::read_to_string(&md).expect("md oku").contains("## Özet"));
        assert!(fs::read_to_string(&js)
            .expect("json oku")
            .contains("\"sema\""));
        let _ = fs::remove_dir_all(&kok);
    }

    #[test]
    fn grup_bosken_markdown_bulunamaz_diyor() {
        let (tarama, motor) = ornek_tarama();
        let bos = MotorSonucu {
            gruplar: Vec::new(),
            sayac: motor.sayac,
            iptal_edildi: false,
        };
        let rapor = rapor_olet(&tarama, &bos, Vec::new());
        assert!(markdown_uret(&rapor).contains("Birebir kopya bulunmadı"));
    }

    #[test]
    fn iptal_edilen_tarama_raporda_isaretlenir() {
        let (tarama, motor) = ornek_tarama();
        let iptalli = MotorSonucu {
            gruplar: motor.gruplar.clone(),
            sayac: motor.sayac,
            iptal_edildi: true,
        };
        let rapor = rapor_olet(&tarama, &iptalli, Vec::new());
        assert!(rapor.iptal_edildi);
        assert!(markdown_uret(&rapor).contains("İPTAL EDİLDİ"));
    }

    #[test]
    fn motor_bos_girdide_hata_uretmez() {
        let tarama = Tarama {
            kok: PathBuf::from("/yok"),
            iptal_edildi: false,
            ..Tarama::default()
        };
        let mut kuyruk = crate::karma::AkisKuyrugu::yeni(64, 2).expect("kuyruk");
        let mut geri = |_: &crate::motor::Ilerleme| {};
        let sonuc = gruplari_uret(
            &tarama,
            &Kurallar::varsayilan(),
            &mut kuyruk,
            &IptalBayragi::yeni(),
            &mut geri,
        )
        .expect("bos tarama");
        assert!(sonuc.gruplar.is_empty());
    }
}
