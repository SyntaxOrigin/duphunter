//! Kademeli aday üretimi, kopya grupları ve kullanıcı kararları.
//!
//! Üç aşama vardır ve her aşama bir öncekinin aday kümesini daraltır:
//!
//! 1. **Boyut gruplama** — hiç okuma yapılmaz, yalnızca meta veri karşılaştırılır.
//! 2. **Hızlı ön eleme** — dosyanın baş ve son penceresi okunur; CRC32 ve BLAKE3
//!    önizleme özetleri eşleşmeyenler elenir.
//! 3. **Tam akış karması** — yalnızca 2. aşamada eşleşenler için BLAKE3
//!    üretilir; "birebir kopya" iddiası yalnızca bu aşamanın ürettiği kanıttır.
//!
//! CRC32 tek başına asla eşleşme kanıtı sayılmaz; yalnızca okuma tasarrufu sağlar.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::gezgin::DosyaMeta;
use crate::karma::KARMA_UZUNLUGU;
use crate::kurallar::Kurallar;

/// 2. aşamanın ürettiği hızlı ön eleme kaydı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnizlemeKaydi {
    /// Dosya meta verisi.
    pub meta: DosyaMeta,
    /// Baş ve son pencerenin CRC32 toplamı.
    pub crc32: u32,
    /// Baş pencerenin BLAKE3 özeti.
    pub ilk: [u8; KARMA_UZUNLUGU],
    /// Son pencerenin BLAKE3 özeti.
    pub son: [u8; KARMA_UZUNLUGU],
}

/// 3. aşama sonrası elde edilen, tam kanıtlı dosya kaydı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KanitliDosya {
    /// Dosyanın tam yolu.
    pub yol: PathBuf,
    /// Dosyanın bayt cinsinden boyutu.
    pub boyut: u64,
    /// 2. aşamadaki hızlı ön eleme sağlaması.
    pub crc32: u32,
    /// Dosyanın tamamının BLAKE3 özeti: birebir kopya kanıtı.
    pub tam_karma: [u8; KARMA_UZUNLUGU],
}

/// Aynı tam karmayı paylaşan bir veya daha çok dosyanın kümesi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grup {
    /// Grup sıra numarası (rapor ve kararlarda kullanılır).
    pub kimlik: usize,
    /// Üyelerin ortak boyutu.
    pub boyut: u64,
    /// Grup üyeleri (yol sırasına göre).
    pub uyeler: Vec<KanitliDosya>,
}

impl Grup {
    /// Korunacak üye: kullanıcı bir yol belirtmediyse ilk üye.
    pub fn varsayilan_koru(&self) -> Option<&KanitliDosya> {
        self.uyeler.first()
    }

    /// Korunacak üyeyi belirtilen yollardan seçer; bulunamazsa ilk üye.
    ///
    /// `asama3_gruplar` yalnızca en az iki üyeli gruplar ürettiği için
    /// kuyruk boştur; yine de `Option` dönülür.
    pub fn koru(&self, korunacak: &[PathBuf]) -> Option<&KanitliDosya> {
        for aday in &self.uyeler {
            if korunacak.iter().any(|k| k == &aday.yol) {
                return Some(aday);
            }
        }
        self.varsayilan_koru()
    }

    /// Bu grup temizlense geri kazanılacak bayt: `(üye sayısı - 1) × boyut`.
    pub fn kazanc_bayt(&self) -> u64 {
        if self.uyeler.len() < 2 {
            return 0;
        }
        (self.uyeler.len() as u64 - 1) * self.boyut
    }
}

/// Bir dosya için alınan kararın türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KararTuru {
    /// Korunacak kopya.
    Koru,
    /// Kurtarma arşivine kopyalanacak aday.
    Arsivle,
    /// Kural gereği hiç dokunulmayacak.
    Asla,
}

/// Kullanıcı adına üretilen, gerekçeli karar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Karar {
    /// Kararın verildiği yol.
    pub yol: PathBuf,
    /// Kararın ait olduğu grup.
    pub grup_kimlik: usize,
    /// Kararın türü.
    pub tur: KararTuru,
    /// Raporda gösterilecek gerekçe metni.
    pub gerekce: String,
}

/// 1. aşama: aynı boyuta sahip dosyaları gruplar, tekil boyutları eler.
///
/// Hiçbir dosya okunmaz; yalnızca meta veri karşılaştırılır.
pub fn asama1_boyut_gruplari(dosyalar: &[DosyaMeta]) -> Vec<Vec<DosyaMeta>> {
    let mut harita: BTreeMap<u64, Vec<DosyaMeta>> = BTreeMap::new();
    for meta in dosyalar {
        harita.entry(meta.boyut).or_default().push(meta.clone());
    }
    harita.into_values().filter(|grup| grup.len() > 1).collect()
}

/// 2. aşama anahtarı: boyut, CRC32 ve iki önizleme özeti birlikte eşleşmelidir.
type OnizlemeAnahtari = (u64, u32, [u8; KARMA_UZUNLUGU], [u8; KARMA_UZUNLUGU]);

/// 2. aşama: ön eleme kayıtlarını anahtarına göre gruplar, tekil olanları eler.
pub fn asama2_onizleme_gruplari(kayitlar: &[OnizlemeKaydi]) -> Vec<Vec<OnizlemeKaydi>> {
    let mut harita: BTreeMap<OnizlemeAnahtari, Vec<OnizlemeKaydi>> = BTreeMap::new();
    for kayit in kayitlar {
        let anahtar = (kayit.meta.boyut, kayit.crc32, kayit.ilk, kayit.son);
        harita.entry(anahtar).or_default().push(kayit.clone());
    }
    harita.into_values().filter(|grup| grup.len() > 1).collect()
}

/// 3. aşama: tam akış karması aynı olan kayıtlardan kopya grupları üretir.
pub fn asama3_gruplar(kayitlar: &[KanitliDosya]) -> Vec<Grup> {
    let mut harita: BTreeMap<(u64, [u8; KARMA_UZUNLUGU]), Vec<KanitliDosya>> = BTreeMap::new();
    for kayit in kayitlar {
        harita
            .entry((kayit.boyut, kayit.tam_karma))
            .or_default()
            .push(kayit.clone());
    }
    let mut gruplar: Vec<Grup> = harita
        .into_values()
        .filter(|grup| grup.len() > 1)
        .map(|mut uyeler| {
            uyeler.sort_by(|a, b| a.yol.cmp(&b.yol));
            Grup {
                kimlik: 0,
                boyut: uyeler[0].boyut,
                uyeler,
            }
        })
        .collect();
    for (sira, grup) in gruplar.iter_mut().enumerate() {
        grup.kimlik = sira;
    }
    gruplar
}

/// Tüm gruplardan geri kazanılacak toplam bayt.
pub fn toplam_kazanc(gruplar: &[Grup]) -> u64 {
    gruplar.iter().map(Grup::kazanc_bayt).sum()
}

/// Gruptaki her üye için gerekçeli karar üretir.
///
/// Varsayılan seçim boş değildir ama yalnızca bir kopya korunur: kullanıcı
/// `korunacak` listesinden bir yol belirtmezse her grupta ilk üye korunur,
/// kalan üyeler arşiv adayı olur. Korunacak uzantılar hiçbir koşulda aday olmaz.
pub fn kararlar_uret(gruplar: &[Grup], kurallar: &Kurallar, korunacak: &[PathBuf]) -> Vec<Karar> {
    let mut kararlar = Vec::new();
    for grup in gruplar {
        let kutu = grup.koru(korunacak).map(|k| k.yol.clone());
        for uye in &grup.uyeler {
            if Some(&uye.yol) == kutu.as_ref() {
                kararlar.push(Karar {
                    yol: uye.yol.clone(),
                    grup_kimlik: grup.kimlik,
                    tur: KararTuru::Koru,
                    gerekce: "grup içinde korunacak kopya olarak seçildi".to_string(),
                });
                continue;
            }
            let ad = uye
                .yol
                .file_name()
                .map(|a| a.to_string_lossy().to_string())
                .unwrap_or_default();
            if kurallar.korunuyor_mu(&ad) {
                kararlar.push(Karar {
                    yol: uye.yol.clone(),
                    grup_kimlik: grup.kimlik,
                    tur: KararTuru::Asla,
                    gerekce: format!("korunacak uzantı: .{}", ad),
                });
                continue;
            }
            kararlar.push(Karar {
                yol: uye.yol.clone(),
                grup_kimlik: grup.kimlik,
                tur: KararTuru::Arsivle,
                gerekce: format!(
                    "{} bayt, {} üyesiyle aynı BLAKE3 karmasına sahip",
                    uye.boyut,
                    grup.uyeler.len()
                ),
            });
        }
    }
    kararlar
}

#[cfg(test)]
// Gerekçe: expect/unwrap yalnızca test içinde kullanılır ve testin
// başarısızlık mesajıdır. Üretim kodunda bu lintler açıktır
// (crate seviyesinde clippy::unwrap_used/clippy::expect_used).
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn meta(yol: &str, boyut: u64) -> DosyaMeta {
        DosyaMeta {
            yol: PathBuf::from(yol),
            boyut,
            gizli: false,
            salt_okunur: false,
        }
    }

    fn kanitli(yol: &str, boyut: u64, k: u8) -> KanitliDosya {
        KanitliDosya {
            yol: PathBuf::from(yol),
            boyut,
            crc32: u32::from(k) * 7,
            tam_karma: [k; KARMA_UZUNLUGU],
        }
    }

    fn onizleme(yol: &str, boyut: u64, k: u8) -> OnizlemeKaydi {
        OnizlemeKaydi {
            meta: meta(yol, boyut),
            crc32: u32::from(k) * 7,
            ilk: [k; KARMA_UZUNLUGU],
            son: [k; KARMA_UZUNLUGU],
        }
    }

    #[test]
    fn asama1_tekil_boyutlari_eler() {
        let liste = vec![meta("a", 10), meta("b", 10), meta("c", 20)];
        let gruplar = asama1_boyut_gruplari(&liste);
        assert_eq!(gruplar.len(), 1);
        assert_eq!(gruplar[0].len(), 2);
    }

    #[test]
    fn asama1_hicbir_dosya_okumaz_ve_boyut_esittir() {
        let liste = vec![meta("a", 10), meta("b", 10), meta("c", 20)];
        for grup in asama1_boyut_gruplari(&liste) {
            let ilk = grup[0].boyut;
            for uye in &grup {
                assert_eq!(uye.boyut, ilk);
            }
        }
    }

    #[test]
    fn asama2_cakisma_senaryosunu_gecir_ve_ayirir() {
        // Ayni boyut ve ayni CRC32/onizleme, farkli icerik: onizleme ayni kalir.
        let liste = vec![
            onizleme("a", 4096, 1),
            onizleme("b", 4096, 1),
            onizleme("c", 4096, 2),
        ];
        let gruplar = asama2_onizleme_gruplari(&liste);
        assert_eq!(gruplar.len(), 1);
        assert_eq!(gruplar[0].len(), 2);
    }

    #[test]
    fn asama2_farkli_onizleme_ayirir() {
        let liste = vec![onizleme("a", 4096, 1), onizleme("b", 4096, 2)];
        assert!(asama2_onizleme_gruplari(&liste).is_empty());
    }

    #[test]
    fn asama3_tam_karma_ayik_ayri_ayirir() {
        let liste = vec![
            kanitli("a", 10, 1),
            kanitli("b", 10, 1),
            kanitli("c", 10, 2),
        ];
        let gruplar = asama3_gruplar(&liste);
        assert_eq!(gruplar.len(), 1);
        assert_eq!(gruplar[0].uyeler.len(), 2);
    }

    #[test]
    fn grup_kazanci_uyeden_bir_eksik_boyuttur() {
        let gruplar = asama3_gruplar(&[
            kanitli("a", 100, 1),
            kanitli("b", 100, 1),
            kanitli("c", 100, 1),
        ]);
        assert_eq!(gruplar[0].kazanc_bayt(), 200);
        assert_eq!(toplam_kazanc(&gruplar), 200);
    }

    #[test]
    fn grup_kimligi_sifirdan_baslar_ve_ardisiktir() {
        let liste = vec![
            kanitli("a", 1, 1),
            kanitli("b", 1, 1),
            kanitli("c", 2, 2),
            kanitli("d", 2, 2),
        ];
        let gruplar = asama3_gruplar(&liste);
        for (sira, grup) in gruplar.iter().enumerate() {
            assert_eq!(grup.kimlik, sira);
        }
    }

    #[test]
    fn varsayilan_koru_ilk_uyedir() {
        let gruplar = asama3_gruplar(&[kanitli("a", 1, 1), kanitli("b", 1, 1)]);
        match gruplar[0].koru(&[]) {
            Some(u) => assert_eq!(u.yol, PathBuf::from("a")),
            None => panic!("koru secimi yapilmali"),
        }
    }

    #[test]
    fn belirtilen_yol_koru_olur() {
        let gruplar = asama3_gruplar(&[kanitli("a", 1, 1), kanitli("b", 1, 1)]);
        let secim = vec![PathBuf::from("b")];
        match gruplar[0].koru(&secim) {
            Some(u) => assert_eq!(u.yol, PathBuf::from("b")),
            None => panic!("koru secimi yapilmali"),
        }
    }

    #[test]
    fn kararlar_bir_koru_ve_digerleri_arsiv_adayi_uretir() {
        let gruplar = asama3_gruplar(&[kanitli("a", 10, 1), kanitli("b", 10, 1)]);
        let kararlar = kararlar_uret(&gruplar, &Kurallar::varsayilan(), &[]);
        assert_eq!(kararlar.len(), 2);
        assert_eq!(kararlar[0].tur, KararTuru::Koru);
        assert_eq!(kararlar[1].tur, KararTuru::Arsivle);
    }

    #[test]
    fn korunacak_uzanti_kurali_karari_engeller() {
        let mut kurallar = Kurallar::varsayilan();
        kurallar.korunacak_uzantilar.insert("keep".to_string());
        // Korunan (ilk) üye "a.txt"; ikinci üye korunacak uzantılı "b.keep".
        let gruplar = asama3_gruplar(&[kanitli("a.txt", 10, 1), kanitli("b.keep", 10, 1)]);
        let kararlar = kararlar_uret(&gruplar, &kurallar, &[]);
        let asla = kararlar.iter().find(|k| k.tur == KararTuru::Asla);
        assert!(asla.is_some(), "korunacak uzantili dosya asla olmali");
    }

    #[test]
    fn korunacak_uzanti_kurali_kapatilinca_arsiv_adayi_olur() {
        let mut kurallar = Kurallar::varsayilan();
        kurallar.korunacak_uzantilar.insert("keep".to_string());
        let gruplar = asama3_gruplar(&[kanitli("a.keep", 10, 1), kanitli("b.txt", 10, 1)]);
        let kararlar = kararlar_uret(&gruplar, &kurallar, &[PathBuf::from("a.keep")]);
        assert!(kararlar.iter().all(|k| k.tur != KararTuru::Asla));
        assert_eq!(
            kararlar
                .iter()
                .filter(|k| k.tur == KararTuru::Arsivle)
                .count(),
            1
        );
    }

    #[test]
    fn bos_grupta_kazanc_sifirdir() {
        let grup = Grup {
            kimlik: 0,
            boyut: 5,
            uyeler: Vec::new(),
        };
        assert_eq!(grup.kazanc_bayt(), 0);
    }

    #[test]
    fn kutu_yolu_gruptadir() {
        let gruplar = asama3_gruplar(&[kanitli("a", 1, 1), kanitli("b", 1, 1)]);
        match gruplar[0].koru(&[]) {
            Some(uye) => assert!(gruplar[0].uyeler.iter().any(|u| u.yol == uye.yol)),
            None => panic!("koru secimi yapilmali"),
        }
    }

    #[test]
    fn karar_gerekcesi_boyutu_icerir() {
        let gruplar = asama3_gruplar(&[kanitli("a", 777, 1), kanitli("b", 777, 1)]);
        let kararlar = kararlar_uret(&gruplar, &Kurallar::varsayilan(), &[]);
        let arsiv = kararlar
            .iter()
            .find(|k| k.tur == KararTuru::Arsivle)
            .expect("arsiv karari");
        assert!(arsiv.gerekce.contains("777"), "{}", arsiv.gerekce);
    }
}
