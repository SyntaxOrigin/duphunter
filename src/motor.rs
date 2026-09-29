//! Tarama motoru: gezgin, akış karması ve gruplayıcıyı birleştirir.
//!
//! Motor üç aşamayı sırayla çalıştırır ve her aşamanın okuduğu baytı
//! sayar. Bu sayaç, "kademeli aday üretimi tam okumayı ne kadar azaltıyor"
//! sorusunun ölçülebilir yanıtıdır ve rapora yazılır.

use std::path::Path;

use crate::gezgin::{self, DosyaMeta, IptalBayragi, Tarama};
use crate::grup::{
    asama1_boyut_gruplari, asama2_onizleme_gruplari, asama3_gruplar, Grup, KanitliDosya,
    OnizlemeKaydi,
};
use crate::hata::Sonuc;
use crate::karma::{onizleme_hesapla, tam_karma, AkisKuyrugu};
use crate::kurallar::Kurallar;

/// Kademeli üretimin hangi aşamasının çalıştığını belirtir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asama {
    /// 1. aşama: boyuta göre gruplama (okuma yok).
    Klasorleme,
    /// 2. aşama: baş/son penceresi ön elemesi.
    Onizleme,
    /// 3. aşama: tam akış BLAKE3.
    TamKarma,
}

/// İlerleme geri çağrımına verilen durum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ilerleme {
    /// Çalışan aşama.
    pub asama: Asama,
    /// Bu aşamada işlenen öğe sayısı.
    pub islenen: usize,
    /// Bu aşamadaki toplam öğe sayısı.
    pub toplam: usize,
}

/// Motorun ilerleme bildirimi için kullandığı geri çağırım türü.
pub type GeriCagirim<'a> = dyn FnMut(&Ilerleme) + 'a;

/// Aşama bazında okuma sayacı.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OkumaSayaci {
    /// 1. aşamaya giren dosya sayısı.
    pub asama1_dosya: usize,
    /// 2. aşamada önizleme üretilen dosya sayısı.
    pub asama2_dosya: usize,
    /// 3. aşamada tam karma üretilen dosya sayısı.
    pub asama3_dosya: usize,
    /// 2. aşamada okunan toplam bayt.
    pub asama2_bayt: u64,
    /// 3. aşamada okunan toplam bayt.
    pub asama3_bayt: u64,
}

impl OkumaSayaci {
    /// Kademeli üretimde okunan toplam bayt.
    pub fn toplam_bayt(&self) -> u64 {
        self.asama2_bayt + self.asama3_bayt
    }

    /// Tüm dosyaların tamamı okunsaydı okunacak bayta göre okunan oranı.
    pub fn okunan_oran(&self, toplam_boyut: u64) -> f64 {
        if toplam_boyut == 0 {
            return 0.0;
        }
        self.toplam_bayt() as f64 / toplam_boyut as f64
    }
}

/// Üç aşamalı aday üretiminin sonucu.
#[derive(Debug, Clone, Default)]
pub struct MotorSonucu {
    /// Birebir kopya grupları (kanıtlı).
    pub gruplar: Vec<Grup>,
    /// Aşama bazlı okuma sayacı.
    pub sayac: OkumaSayaci,
    /// Tarama iptal edildiği için sonuç eksikse `true`.
    pub iptal_edildi: bool,
}

/// Dizini gezer ve üç aşamalı aday üretimini çalıştırır.
pub fn tara_ve_uret(
    kok: &Path,
    kurallar: &Kurallar,
    kuyruk: &mut AkisKuyrugu,
    iptal: &IptalBayragi,
    ilerleme: &mut GeriCagirim<'_>,
) -> Sonuc<(Tarama, MotorSonucu)> {
    let tarama = gezgin::tara(kok, kurallar, iptal, |yol, sayi| {
        ilerleme(&Ilerleme {
            asama: Asama::Klasorleme,
            islenen: sayi,
            toplam: 0,
        });
        let _ = yol;
    })?;
    let sonuc = gruplari_uret(&tarama, kurallar, kuyruk, iptal, ilerleme)?;
    Ok((tarama, sonuc))
}

/// Var olan bir tarama sonucundan kopya gruplarını üretir.
///
/// Aşamalar şöyle ilerler:
///
/// 1. Boyut gruplama: hiç okuma yapılmaz.
/// 2. Ön eleme: dosyanın baş ve son penceresi okunur.
/// 3. Tam akış BLAKE3: yalnızca 2. aşamada eşleşen dosyalar için.
///
/// İptal bayrağı yükselirse elde edilen kısmi sonuç `iptal_edildi`
/// işaretiyle döner.
pub fn gruplari_uret(
    tarama: &Tarama,
    kurallar: &Kurallar,
    kuyruk: &mut AkisKuyrugu,
    iptal: &IptalBayragi,
    ilerleme: &mut GeriCagirim<'_>,
) -> Sonuc<MotorSonucu> {
    let mut sayac = OkumaSayaci {
        asama1_dosya: tarama.dosyalar.len(),
        ..OkumaSayaci::default()
    };
    ilerleme(&Ilerleme {
        asama: Asama::Klasorleme,
        islenen: sayac.asama1_dosya,
        toplam: sayac.asama1_dosya,
    });

    // --- 1. aşama: boyut gruplama -------------------------------------------------
    let boyut_gruplari = asama1_boyut_gruplari(&tarama.dosyalar);
    let ikinci_adaylar: Vec<DosyaMeta> = boyut_gruplari
        .iter()
        .flat_map(|grup| grup.iter().cloned())
        .collect();
    if iptal.iptal_ildi_mi() {
        return Ok(MotorSonucu {
            gruplar: Vec::new(),
            sayac,
            iptal_edildi: true,
        });
    }

    // --- 2. aşama: hızlı ön eleme ------------------------------------------------
    let mut onizlemeler: Vec<OnizlemeKaydi> = Vec::new();
    let toplam2 = ikinci_adaylar.len();
    for (sira, meta) in ikinci_adaylar.iter().enumerate() {
        if iptal.iptal_ildi_mi() {
            return Ok(MotorSonucu {
                gruplar: Vec::new(),
                sayac,
                iptal_edildi: true,
            });
        }
        ilerleme(&Ilerleme {
            asama: Asama::Onizleme,
            islenen: sira,
            toplam: toplam2,
        });
        let onizleme = onizleme_hesapla(&meta.yol, kurallar.onizleme_bayt, kuyruk)?;
        sayac.asama2_bayt += onizleme.okunan_bayt;
        onizlemeler.push(OnizlemeKaydi {
            meta: meta.clone(),
            crc32: onizleme.crc32,
            ilk: onizleme.ilk,
            son: onizleme.son,
        });
    }
    sayac.asama2_dosya = onizlemeler.len();
    let onizleme_gruplari = asama2_onizleme_gruplari(&onizlemeler);
    let ucuncu_adaylar: Vec<OnizlemeKaydi> = onizleme_gruplari
        .into_iter()
        .flat_map(|grup| grup.into_iter())
        .collect();
    if iptal.iptal_ildi_mi() {
        return Ok(MotorSonucu {
            gruplar: Vec::new(),
            sayac,
            iptal_edildi: true,
        });
    }

    // --- 3. aşama: tam akış karması ---------------------------------------------
    let mut kanitlilar: Vec<KanitliDosya> = Vec::new();
    let toplam3 = ucuncu_adaylar.len();
    for (sira, aday) in ucuncu_adaylar.iter().enumerate() {
        if iptal.iptal_ildi_mi() {
            return Ok(MotorSonucu {
                gruplar: Vec::new(),
                sayac,
                iptal_edildi: true,
            });
        }
        ilerleme(&Ilerleme {
            asama: Asama::TamKarma,
            islenen: sira,
            toplam: toplam3,
        });
        let boyut = aday.meta.boyut;
        let yol = aday.meta.yol.clone();
        let crc32 = aday.crc32;
        let karma = tam_karma(&yol, kuyruk)?;
        sayac.asama3_bayt += boyut;
        kanitlilar.push(KanitliDosya {
            yol,
            boyut,
            crc32,
            tam_karma: karma,
        });
    }
    sayac.asama3_dosya = kanitlilar.len();
    ilerleme(&Ilerleme {
        asama: Asama::TamKarma,
        islenen: toplam3,
        toplam: toplam3,
    });

    let gruplar = asama3_gruplar(&kanitlilar);
    Ok(MotorSonucu {
        gruplar,
        sayac,
        iptal_edildi: false,
    })
}

#[cfg(test)]
// Gerekçe: expect/unwrap yalnızca test içinde kullanılır ve testin
// başarısızlık mesajıdır. Üretim kodunda bu lintler açıktır
// (crate seviyesinde clippy::unwrap_used/clippy::expect_used).
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct Sahne {
        kok: PathBuf,
    }

    impl Sahne {
        fn yeni(etiket: &str) -> Self {
            let kok = std::env::temp_dir().join(format!("duphunter-motor-{}", etiket));
            let _ = fs::remove_dir_all(&kok);
            fs::create_dir_all(&kok).expect("gecici dizin");
            Self { kok }
        }
        fn dosya(&self, ad: &str, veri: &[u8]) -> PathBuf {
            let p = self.kok.join(ad);
            fs::create_dir_all(p.parent().expect("ebeveyn")).expect("dizin");
            fs::write(&p, veri).expect("yaz");
            p
        }
    }

    impl Drop for Sahne {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.kok);
        }
    }

    fn sessiz_kuyruk() -> AkisKuyrugu {
        AkisKuyrugu::yeni(4096, 2).expect("kuyruk")
    }

    #[test]
    fn motor_ayni_icerigi_kopya_grubuna_alir() {
        let s = Sahne::yeni("grup");
        s.dosya("a.txt", b"ayni icerik");
        s.dosya("b.txt", b"ayni icerik");
        s.dosya("c.txt", b"farkli icerik!");
        let mut kuyruk = sessiz_kuyruk();
        let (tarama, sonuc) = tara_ve_uret(
            &s.kok,
            &Kurallar::varsayilan(),
            &mut kuyruk,
            &IptalBayragi::yeni(),
            &mut |_| {},
        )
        .expect("tara");
        assert_eq!(tarama.dosyalar.len(), 3);
        assert_eq!(sonuc.gruplar.len(), 1);
        assert_eq!(sonuc.gruplar[0].uyeler.len(), 2);
    }

    #[test]
    fn motor_tekil_dosyalarda_okuma_yapmaz() {
        let s = Sahne::yeni("tekil");
        s.dosya("a.txt", b"bir");
        s.dosya("b.txt", b"ikiler");
        s.dosya("c.txt", b"uc!?!");
        let mut kuyruk = sessiz_kuyruk();
        let (_, sonuc) = tara_ve_uret(
            &s.kok,
            &Kurallar::varsayilan(),
            &mut kuyruk,
            &IptalBayragi::yeni(),
            &mut |_| {},
        )
        .expect("tara");
        assert!(sonuc.gruplar.is_empty());
        assert_eq!(sonuc.sayac.asama2_dosya, 0, "tekil boyutlar okunmamali");
        assert_eq!(sonuc.sayac.toplam_bayt(), 0);
    }

    #[test]
    fn motor_ayni_boyut_farkli_icerikte_ikinci_asamaya_girer() {
        let s = Sahne::yeni("ayniBoyut");
        s.dosya("a.bin", &vec![1u8; 4096]);
        s.dosya("b.bin", &vec![2u8; 4096]);
        let mut kuyruk = sessiz_kuyruk();
        let (_, sonuc) = tara_ve_uret(
            &s.kok,
            &Kurallar::varsayilan(),
            &mut kuyruk,
            &IptalBayragi::yeni(),
            &mut |_| {},
        )
        .expect("tara");
        assert_eq!(sonuc.sayac.asama2_dosya, 2);
        assert_eq!(
            sonuc.sayac.asama3_dosya, 0,
            "onizleme elemesi sonrasi aday kalmadi"
        );
    }

    #[test]
    fn motor_crc_cakismasinda_ucuncu_asamaya_gecer_ve_ayirir() {
        let s = Sahne::yeni("cakisma");
        // Önizleme penceresi 64 bayt: 4096 baytlık iki dosyanın baş 64 ve son 64
        // baytı aynı, ortası farklıdır. 2. aşama (CRC32 + baş/son BLAKE3) iki
        // dosyayı da aday tutar, 3. aşama (tam BLAKE3) ayırır.
        let mut a = vec![b'A'; 64];
        a.extend_from_slice(&[b'X'; 4096 - 128]);
        a.extend_from_slice(&[b'Z'; 64]);
        let mut b = vec![b'A'; 64];
        b.extend_from_slice(&[b'Y'; 4096 - 128]);
        b.extend_from_slice(&[b'Z'; 64]);
        s.dosya("a.bin", &a);
        s.dosya("b.bin", &b);
        let mut kurallar = Kurallar::varsayilan();
        kurallar.onizleme_bayt = 64;
        let mut kuyruk = sessiz_kuyruk();
        let (_, sonuc) = tara_ve_uret(
            &s.kok,
            &kurallar,
            &mut kuyruk,
            &IptalBayragi::yeni(),
            &mut |_| {},
        )
        .expect("tara");
        assert_eq!(sonuc.sayac.asama2_dosya, 2);
        assert_eq!(sonuc.sayac.asama3_dosya, 2, "cakisma 3. asamaya tasinmali");
        assert!(sonuc.gruplar.is_empty(), "farkli icerik ayni grup olmamali");
    }

    #[test]
    fn motor_ilerleme_uc_asamayi_bildirir() {
        let s = Sahne::yeni("ilerleme");
        s.dosya("a.txt", b"ayni");
        s.dosya("b.txt", b"ayni");
        let mut kuyruk = sessiz_kuyruk();
        let mut gorulenler: Vec<Asama> = Vec::new();
        {
            let mut geri = |i: &Ilerleme| {
                if gorulenler.last() != Some(&i.asama) {
                    gorulenler.push(i.asama);
                }
            };
            let _ = tara_ve_uret(
                &s.kok,
                &Kurallar::varsayilan(),
                &mut kuyruk,
                &IptalBayragi::yeni(),
                &mut geri,
            )
            .expect("tara");
        }
        assert_eq!(gorulenler.first(), Some(&Asama::Klasorleme));
        assert!(gorulenler.contains(&Asama::Onizleme));
        assert!(gorulenler.contains(&Asama::TamKarma));
    }

    #[test]
    fn motor_iptal_edildiginde_yarim_sonuc_dondurur() {
        let s = Sahne::yeni("iptal");
        for i in 0..4 {
            s.dosya(&format!("a{}.txt", i), b"ayni icerik");
        }
        let iptal = IptalBayragi::yeni();
        let iptal2 = iptal.clone();
        let mut kuyruk = sessiz_kuyruk();
        let (_, sonuc) = tara_ve_uret(
            &s.kok,
            &Kurallar::varsayilan(),
            &mut kuyruk,
            &iptal,
            &mut |i| {
                if i.asama == Asama::Onizleme && i.islenen == 0 {
                    iptal2.iptal_et();
                }
            },
        )
        .expect("tara");
        assert!(sonuc.iptal_edildi, "iptal istegi islenmeli");
    }

    #[test]
    fn motor_tarama_sirasinda_hicbir_dosyaya_dokunmaz() {
        let s = Sahne::yeni("dokunma");
        let a = s.dosya("a.txt", b"ayni");
        s.dosya("b.txt", b"ayni");
        let once = fs::read(&a).expect("oku");
        let mut kuyruk = sessiz_kuyruk();
        let _ = tara_ve_uret(
            &s.kok,
            &Kurallar::varsayilan(),
            &mut kuyruk,
            &IptalBayragi::yeni(),
            &mut |_| {},
        )
        .expect("tara");
        assert_eq!(fs::read(&a).expect("oku"), once);
    }

    #[test]
    fn okuma_sayaci_orani_hesaplar() {
        let sayac = OkumaSayaci {
            asama2_bayt: 1000,
            asama3_bayt: 500,
            ..OkumaSayaci::default()
        };
        assert_eq!(sayac.toplam_bayt(), 1500);
        let oran = sayac.okunan_oran(10_000);
        assert!(oran > 0.14 && oran < 0.16, "oran {}", oran);
        assert_eq!(sayac.okunan_oran(0), 0.0);
    }
}
