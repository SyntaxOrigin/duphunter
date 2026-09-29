//! Sabit bellekli akış karması: önizleme penceresi (CRC32) ve tam BLAKE3.
//!
//! Bu modülün tek kuralı vardır: **hiçbir dosya belleğe tamamen alınmaz.**
//! Okuma işlemi `AkisKuyrugu` üzerinden sabit sayıda, sabit boyutlu dilim
//! halinde yapılır; ayrılan bellek `kapasite × tampon_bayt` ile sabittir ve
//! dosya boyutundan bağımsızdır. Bu, MANIFEST.md'deki "1 MB sabit tampon,
//! 4 GB dosya bu bütçeyi bozmaz" gereksiniminin karşılığıdır.

use std::fs::{self, File};
use std::io::{Read, SeekFrom};
use std::path::Path;

use crate::hata::{Hata, Sonuc};
use crate::yol;

/// Varsayılan akış tamponu (raporun önerisi).
pub const VARSAYILAN_TAMPON_BAYT: usize = 1024 * 1024;

/// Varsayılan kuyruk derinliği: iki dilim, okuma ile tüketim örtüşebilir.
pub const VARSAYILAN_KUYRUK_KAPASITESI: usize = 2;

/// BLAKE3 çıktısının bayt cinsinden uzunluğu.
pub const KARMA_UZUNLUGU: usize = 32;

/// Son `N` baytı tutan, ekleme başına sabit maliyetli kayan pencere.
///
/// Baş ve son pencereleri tek geçişte üretmek için kullanılır; dosya
/// küçükse pencere hiç dolmaz ve elde edilen içerik dosyanın kendisidir.
#[derive(Debug, Clone)]
pub struct KayanPencere {
    tampon: Vec<u8>,
    dolu: usize,
    imlec: usize,
}

impl KayanPencere {
    /// Verilen kapasiteyle (bayt) yeni pencere oluşturur.
    ///
    /// Sıfır kapasite reddedilir: sıfır pencere işe yaramaz ve kural
    /// dosyası doğrulamasının atlanması durumunda hatayı gizlerdi.
    pub fn yeni(kapasite: usize) -> Sonuc<Self> {
        if kapasite == 0 {
            return Err(Hata::Parametre {
                ad: "kapasite",
                ayrinti: "kayan pencere kapasitesi sıfır olamaz".to_string(),
            });
        }
        Ok(Self {
            tampon: vec![0u8; kapasite],
            dolu: 0,
            imlec: 0,
        })
    }

    /// Yeni baytları pencereye ekler; gerekiyorsa en eski baytları düşürür.
    pub fn ekle(&mut self, veri: &[u8]) {
        let kap = self.tampon.len();
        if kap == 0 || veri.is_empty() {
            return;
        }
        if self.dolu == kap {
            // Pencere dolu: yalnızca son `kap` bayt saklanır, toplu kopya kullanılır.
            let mut kaynak = veri;
            if kaynak.len() > kap {
                kaynak = &kaynak[kaynak.len() - kap..];
            }
            let ilk = std::cmp::min(kaynak.len(), kap - self.imlec);
            self.tampon[self.imlec..self.imlec + ilk].copy_from_slice(&kaynak[..ilk]);
            if kaynak.len() > ilk {
                self.tampon[..kaynak.len() - ilk].copy_from_slice(&kaynak[ilk..]);
            }
            self.imlec = (self.imlec + kaynak.len()) % kap;
            return;
        }
        for bayt in veri {
            self.tampon[self.imlec] = *bayt;
            self.imlec = (self.imlec + 1) % kap;
            if self.dolu < kap {
                self.dolu += 1;
            }
        }
    }

    /// Penceredeki baytları yazıldıkları sırayla kopyalar.
    pub fn sirali(&self) -> Vec<u8> {
        let kap = self.tampon.len();
        let mut cikti = Vec::with_capacity(self.dolu);
        if self.dolu == 0 || kap == 0 {
            return cikti;
        }
        let bas = (self.imlec + kap - self.dolu) % kap;
        if bas + self.dolu <= kap {
            cikti.extend_from_slice(&self.tampon[bas..bas + self.dolu]);
        } else {
            cikti.extend_from_slice(&self.tampon[bas..]);
            cikti.extend_from_slice(&self.tampon[..self.dolu - (kap - bas)]);
        }
        cikti
    }

    /// Pencerede şu an bulunan bayt sayısı.
    pub fn dolu(&self) -> usize {
        self.dolu
    }

    /// Pencerenin taşıyabileceği en fazla bayt sayısı.
    pub fn kapasite(&self) -> usize {
        self.tampon.len()
    }
}

/// Sabit bellekli okuma kuyruğu.
///
/// `kapasite` adet `tampon_bayt` boyutunda dilim bir kez ayrılır ve her
/// okuma için yeniden kullanılır; ayrılan toplam bellek `ayrilan_bellek()`
/// ile okunabilir ve dosya boyutundan bağımsız olarak sabittir.
#[derive(Debug, Clone)]
pub struct AkisKuyrugu {
    tampon_bayt: usize,
    dilimler: Vec<Vec<u8>>,
    imlec: usize,
    gecen: u64,
}

impl AkisKuyrugu {
    /// Verilen tampon boyutu ve kuyruk derinliğiyle yeni kuyruk oluşturur.
    pub fn yeni(tampon_bayt: usize, kapasite: usize) -> Sonuc<Self> {
        if tampon_bayt == 0 {
            return Err(Hata::Parametre {
                ad: "tampon_bayt",
                ayrinti: "akış tamponu sıfır olamaz".to_string(),
            });
        }
        if kapasite == 0 {
            return Err(Hata::Parametre {
                ad: "kapasite",
                ayrinti: "akış kuyruğu kapasitesi sıfır olamaz".to_string(),
            });
        }
        let mut dilimler = Vec::with_capacity(kapasite);
        for _ in 0..kapasite {
            dilimler.push(vec![0u8; tampon_bayt]);
        }
        Ok(Self {
            tampon_bayt,
            dilimler,
            imlec: 0,
            gecen: 0,
        })
    }

    /// Dilim başına bayt sayısı.
    pub fn tampon_bayt(&self) -> usize {
        self.tampon_bayt
    }

    /// Kuyruktaki dilim sayısı.
    pub fn kapasite(&self) -> usize {
        self.dilimler.len()
    }

    /// Toplam ayrılan tampon belleği (bayt).
    pub fn ayrilan_bellek(&self) -> usize {
        self.tampon_bayt * self.dilimler.len()
    }

    /// Kuyruktan geçen toplam bayt sayısı.
    pub fn gecen_bayt(&self) -> u64 {
        self.gecen
    }

    /// Okuma ve tüketme döngüsünü sabit bellekle yürütür.
    ///
    /// `oku` bir dilim doldurur, `tuket` o dilimi işler; ikisi de hata
    /// döndürebilir. Döngü, okuma 0 bayt döndürdüğünde biter.
    pub fn bosalt<R, T>(&mut self, mut oku: R, mut tuket: T) -> Sonuc<u64>
    where
        R: FnMut(&mut [u8]) -> Sonuc<usize>,
        T: FnMut(&[u8]) -> Sonuc<()>,
    {
        let derinlik = self.dilimler.len();
        let mut toplam = 0u64;
        loop {
            let imlec = self.imlec;
            let okunan = oku(&mut self.dilimler[imlec][..])?;
            if okunan == 0 {
                break;
            }
            toplam += okunan as u64;
            self.gecen += okunan as u64;
            tuket(&self.dilimler[imlec][..okunan])?;
            self.imlec = (imlec + 1) % derinlik;
        }
        Ok(toplam)
    }
}

/// 2. aşamanın ürettiği hızlı ön eleme kanıtı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Onizleme {
    /// Baş ve son pencerenin CRC32 toplamı (yalnız ön eleme içindir).
    pub crc32: u32,
    /// Baş pencerenin BLAKE3 özeti.
    pub ilk: [u8; KARMA_UZUNLUGU],
    /// Son pencerenin BLAKE3 özeti.
    pub son: [u8; KARMA_UZUNLUGU],
    /// Bu önizleme için okunan toplam bayt.
    pub okunan_bayt: u64,
}

/// CRC-32 (ISO 3309) sağlamasını hesaplar.
pub fn crc32(bayt: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(bayt);
    h.finalize()
}

/// Tek bir dizinin CRC32 toplamını hesaplar.
pub fn crc32_parca(bayt: &[u8], onceki: u32) -> u32 {
    let mut h = crc32fast::Hasher::new_with_initial(onceki);
    h.update(bayt);
    h.finalize()
}

/// BLAKE3 özetini küçük harf onaltılık metne çevirir.
pub fn karma_hex(karma: &[u8; KARMA_UZUNLUGU]) -> String {
    let mut metin = String::with_capacity(KARMA_UZUNLUGU * 2);
    for bayt in karma {
        metin.push_str(&format!("{:02x}", bayt));
    }
    metin
}

/// Bir dosyanın baş ve son penceresini okuyup önizleme kanıtı üretir.
///
/// Büyük dosyalarda **yalnızca baş ve son pencereler** okunur: dosya
/// `SeekFrom::End` ile son pencereye atlanır. Bu, 2. aşamanın varlık
/// nedenidir — tam okuma 3. aşamaya ertelenir. Dosya
/// `2 × pencere_bayt` boyutundan küçükse tüm içerik tek geçişte okunur ve
/// bellek iki pencere ile sınırlı kalır.
pub fn onizleme_hesapla(
    dosya: &Path,
    pencere_bayt: usize,
    kuyruk: &mut AkisKuyrugu,
) -> Sonuc<Onizleme> {
    use std::io::Seek;

    let meta = fs::metadata(yol::uzun_yol(dosya))
        .map_err(|kaynak| Hata::io("dosya ölçüsü okuma (önizleme)", dosya, kaynak))?;
    let mut okuyucu = File::open(yol::uzun_yol(dosya))
        .map_err(|kaynak| Hata::io("dosya açma (önizleme)", dosya, kaynak))?;
    let mut okunan = 0u64;

    let (bas, son_veri): (Vec<u8>, Vec<u8>) =
        if meta.len() <= (pencere_bayt as u64).saturating_mul(2) {
            let mut tamam: Vec<u8> = Vec::new();
            kuyruk.bosalt(
                |dilim| {
                    okuyucu
                        .read(dilim)
                        .map_err(|kaynak| Hata::io("dosya okuma (önizleme)", dosya, kaynak))
                },
                |veri| {
                    okunan += veri.len() as u64;
                    tamam.extend_from_slice(veri);
                    Ok(())
                },
            )?;
            let bas_al = std::cmp::min(pencere_bayt, tamam.len());
            let son_bas = tamam.len().saturating_sub(pencere_bayt);
            (tamam[..bas_al].to_vec(), tamam[son_bas..].to_vec())
        } else {
            // Okuma dilimi kalan pencereyle sınırlanır: `bosalt` 0 bayt dönene
            // kadar sürer, bu yüzden "yeter bayt toplandı" durumu okuma
            // tarafına da bildirilmelidir.
            let mut kalan = pencere_bayt;
            let mut ilk: Vec<u8> = Vec::with_capacity(pencere_bayt);
            kuyruk.bosalt(
                |dilim| {
                    if kalan == 0 {
                        return Ok(0);
                    }
                    let al = std::cmp::min(dilim.len(), kalan);
                    let n = okuyucu
                        .read(&mut dilim[..al])
                        .map_err(|kaynak| Hata::io("dosya okuma (önizleme)", dosya, kaynak))?;
                    kalan -= n;
                    Ok(n)
                },
                |veri| {
                    okunan += veri.len() as u64;
                    ilk.extend_from_slice(veri);
                    Ok(())
                },
            )?;
            okuyucu
                .seek(SeekFrom::End(-(pencere_bayt as i64)))
                .map_err(|kaynak| Hata::io("dosya sonuna atlama (önizleme)", dosya, kaynak))?;
            let mut kalan = pencere_bayt;
            let mut son: Vec<u8> = Vec::with_capacity(pencere_bayt);
            kuyruk.bosalt(
                |dilim| {
                    if kalan == 0 {
                        return Ok(0);
                    }
                    let al = std::cmp::min(dilim.len(), kalan);
                    let n = okuyucu
                        .read(&mut dilim[..al])
                        .map_err(|kaynak| Hata::io("dosya okuma (önizleme)", dosya, kaynak))?;
                    kalan -= n;
                    Ok(n)
                },
                |veri| {
                    okunan += veri.len() as u64;
                    son.extend_from_slice(veri);
                    Ok(())
                },
            )?;
            (ilk, son)
        };

    let toplam = crc32_parca(&son_veri, crc32(&bas));
    Ok(Onizleme {
        crc32: toplam,
        ilk: *blake3::hash(&bas).as_bytes(),
        son: *blake3::hash(&son_veri).as_bytes(),
        okunan_bayt: okunan,
    })
}

/// Bir dosyanın tamamını sabit bellekle okuyup BLAKE3 özetini üretir.
pub fn tam_karma(dosya: &Path, kuyruk: &mut AkisKuyrugu) -> Sonuc<[u8; KARMA_UZUNLUGU]> {
    let mut okuyucu = File::open(yol::uzun_yol(dosya))
        .map_err(|kaynak| Hata::io("dosya açma (tam karma)", dosya, kaynak))?;
    let mut karma = blake3::Hasher::new();
    kuyruk.bosalt(
        |dilim| {
            okuyucu
                .read(dilim)
                .map_err(|kaynak| Hata::io("dosya okuma (tam karma)", dosya, kaynak))
        },
        |veri| {
            karma.update(veri);
            Ok(())
        },
    )?;
    Ok(*karma.finalize().as_bytes())
}

/// Akış halinde kopyalama sırasında üretilen bütünlük kanıtı.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KopyaKaniti {
    /// Kopyalanan bayt sayısı.
    pub boyut: u64,
    /// Kopyalanan içeriğin tam BLAKE3 özeti.
    pub blake3: [u8; KARMA_UZUNLUGU],
    /// Kopyalanan içeriğin CRC32 sağlaması.
    pub crc32: u32,
}

/// Dosyayı hedefe akış halinde kopyalar ve kopyalama sırasında BLAKE3 ve CRC32 üretir.
///
/// Kaynak dosya hiçbir koşulda silinmez; hedef yol var olan bir dosyayı
/// üzerine yazmadan önce `File::create` tarafından kesilir, bu yüzden çağıran
/// taraf hedefin boş olduğunu doğrulamalıdır.
pub fn dosya_kopyala_ve_karmala(
    kaynak: &Path,
    hedef: &Path,
    kuyruk: &mut AkisKuyrugu,
) -> Sonuc<KopyaKaniti> {
    let mut okuyucu = File::open(yol::uzun_yol(kaynak))
        .map_err(|hata| Hata::io("arşiv: kaynak açma", kaynak, hata))?;
    let mut yazici = File::create(yol::uzun_yol(hedef))
        .map_err(|hata| Hata::io("arşiv: hedef oluşturma", hedef, hata))?;
    let mut karma = blake3::Hasher::new();
    let mut crc = 0u32;
    let toplam = kuyruk.bosalt(
        |dilim| {
            okuyucu
                .read(dilim)
                .map_err(|hata| Hata::io("arşiv: kaynak okuma", kaynak, hata))
        },
        |veri| {
            use std::io::Write;
            karma.update(veri);
            crc = crc32_parca(veri, crc);
            yazici
                .write_all(veri)
                .map_err(|hata| Hata::io("arşiv: hedefe yazma", hedef, hata))
        },
    )?;
    use std::io::Write;
    yazici
        .flush()
        .map_err(|hata| Hata::io("arşiv: hedef boşaltma", hedef, hata))?;
    Ok(KopyaKaniti {
        boyut: toplam,
        blake3: *karma.finalize().as_bytes(),
        crc32: crc,
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

    /// BLAKE3 resmi test vektörlerinin girdisi: 251 baytlık tekrar eden dizi.
    fn resmi_girdi(uzunluk: usize) -> Vec<u8> {
        (0..uzunluk).map(|i| (i % 251) as u8).collect()
    }

    fn gecici_dosya(etiket: &str, veri: &[u8]) -> std::path::PathBuf {
        // Dizin yalnızca yoksa oluşturulur: aynı modüldeki testler paralel
        // çalıştığı için hazır dizini her çağrıda silmek diğerlerinin
        // dosyalarını yok ederdi.
        let kok = std::env::temp_dir().join("duphunter-karma-testleri");
        if !kok.is_dir() {
            fs::create_dir_all(&kok).expect("gecici dizin");
        }
        let dosya = kok.join(etiket);
        fs::write(&dosya, veri).expect("gecici dosya yaz");
        dosya
    }

    #[test]
    fn crc32_kontrol_degeri_iso3309() {
        // ISO 3309 "check" değeri: CRC-32("123456789") = 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn crc32_parca_toplamla_ayni_sonucu_verir() {
        let veri = b"123456789";
        let tek = crc32(veri);
        let parca = crc32_parca(&veri[5..], crc32(&veri[..5]));
        assert_eq!(tek, parca);
    }

    #[test]
    fn kayan_pencere_sondaki_baytlari_tutar() {
        let mut p = KayanPencere::yeni(4).expect("pencere");
        p.ekle(b"abcdef");
        assert_eq!(p.sirali(), b"cdef".to_vec());
        assert_eq!(p.dolu(), 4);
    }

    #[test]
    fn kayan_pencere_kisa_girdide_kendisi_kalir() {
        let mut p = KayanPencere::yeni(8).expect("pencere");
        p.ekle(b"ab");
        assert_eq!(p.sirali(), b"ab".to_vec());
        assert_eq!(p.dolu(), 2);
        assert_eq!(p.kapasite(), 8);
    }

    #[test]
    fn kayan_pencere_sarma_noktasini_dogru_gecer() {
        let mut p = KayanPencere::yeni(4).expect("pencere");
        for adim in 0..7 {
            p.ekle(&[adim]);
        }
        assert_eq!(p.sirali(), b"\x03\x04\x05\x06".to_vec());
    }

    #[test]
    fn sifir_kapasiteli_pencere_reddedilir() {
        assert!(KayanPencere::yeni(0).is_err());
    }

    #[test]
    fn akis_kuyrugu_bellek_miktari_sabit() {
        let k = AkisKuyrugu::yeni(1024, 2).expect("kuyruk");
        assert_eq!(k.ayrilan_bellek(), 2048);
        assert_eq!(k.tampon_bayt(), 1024);
        assert_eq!(k.kapasite(), 2);
        assert_eq!(k.gecen_bayt(), 0);
    }

    #[test]
    fn akis_kuyrugu_sifir_tamponu_reddeder() {
        assert!(AkisKuyrugu::yeni(0, 2).is_err());
        assert!(AkisKuyrugu::yeni(16, 0).is_err());
    }

    #[test]
    fn akis_kuyrugu_dilimleri_yeniden_kullanir() {
        let mut k = AkisKuyrugu::yeni(4, 2).expect("kuyruk");
        let veri = b"abcdefghij";
        let mut sayac = 0usize;
        let toplam = k
            .bosalt(
                |dilim| {
                    let n = std::cmp::min(dilim.len(), veri.len() - sayac);
                    dilim[..n].copy_from_slice(&veri[sayac..sayac + n]);
                    sayac += n;
                    Ok(n)
                },
                |_blok| Ok(()),
            )
            .expect("bosalt");
        assert_eq!(toplam, 10);
        assert_eq!(k.gecen_bayt(), 10);
        assert_eq!(k.ayrilan_bellek(), 8, "bellek dosya boyutunca artmamali");
    }

    #[test]
    fn tam_karma_resmi_blake3_vektoru_0_bayt() {
        let dosya = gecici_dosya("bos.bin", b"");
        let mut k = AkisKuyrugu::yeni(64, 2).expect("kuyruk");
        let karma = tam_karma(&dosya, &mut k).expect("karma");
        assert_eq!(
            karma_hex(&karma),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn tam_karma_resmi_blake3_vektoru_1024_bayt() {
        let veri = resmi_girdi(1024);
        let dosya = gecici_dosya("1024.bin", &veri);
        let mut k = AkisKuyrugu::yeni(128, 2).expect("kuyruk");
        let karma = tam_karma(&dosya, &mut k).expect("karma");
        assert_eq!(
            karma_hex(&karma),
            "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7"
        );
    }

    #[test]
    fn tam_karma_resmi_blake3_vektoru_102400_bayt_kucuk_tamponla() {
        let veri = resmi_girdi(102_400);
        let dosya = gecici_dosya("102400.bin", &veri);
        let mut k = AkisKuyrugu::yeni(1024, 2).expect("kuyruk");
        let karma = tam_karma(&dosya, &mut k).expect("karma");
        assert_eq!(
            karma_hex(&karma),
            "bc3e3d41a1146b069abffad3c0d44860cf664390afce4d9661f7902e7943e085"
        );
        assert_eq!(k.ayrilan_bellek(), 2048);
        assert_eq!(k.gecen_bayt(), 102_400);
    }

    #[test]
    fn onizleme_kucuk_dosyada_tam_icerigi_okur() {
        let dosya = gecici_dosya("kucuk.txt", b"123456789");
        let mut k = AkisKuyrugu::yeni(64, 2).expect("kuyruk");
        let onizleme = onizleme_hesapla(&dosya, 64, &mut k).expect("onizleme");
        assert_eq!(onizleme.okunan_bayt, 9);
        assert_eq!(onizleme.ilk, *blake3::hash(b"123456789").as_bytes());
        // Dosya pencereden küçükse baş ve son aynı içeriktir; CRC iki kez işlenir.
        assert_eq!(
            onizleme.crc32,
            crc32_parca(b"123456789", crc32(b"123456789"))
        );
    }

    #[test]
    fn onizleme_buyuk_dosyada_basi_sonu_ayirir_ve_tam_okumaz() {
        let mut veri = vec![b'B'; 1000];
        veri[..100].fill(b'A');
        veri[900..].fill(b'Z');
        let dosya = gecici_dosya("buyuk.bin", &veri);
        let mut k = AkisKuyrugu::yeni(64, 2).expect("kuyruk");
        let onizleme = onizleme_hesapla(&dosya, 100, &mut k).expect("onizleme");
        assert_eq!(
            onizleme.okunan_bayt, 200,
            "yalnizca bas ve son pencere okunmali"
        );
        assert_eq!(onizleme.ilk, *blake3::hash(&veri[..100]).as_bytes());
        assert_eq!(onizleme.son, *blake3::hash(&veri[900..]).as_bytes());
    }

    #[test]
    fn onizleme_ayni_icerikte_ayni_kanit_verir() {
        let veri = resmi_girdi(4096);
        let a = gecici_dosya("a.bin", &veri);
        let b = gecici_dosya("b.bin", &veri);
        let mut k = AkisKuyrugu::yeni(256, 2).expect("kuyruk");
        let oa = onizleme_hesapla(&a, 256, &mut k).expect("onizleme a");
        let ob = onizleme_hesapla(&b, 256, &mut k).expect("onizleme b");
        assert_eq!(oa.crc32, ob.crc32);
        assert_eq!(oa.ilk, ob.ilk);
        assert_eq!(oa.son, ob.son);
    }

    #[test]
    fn kopyalama_karma_ve_boyut_dondurur_kaynagi_silmez() {
        let veri = resmi_girdi(3000);
        let kok = std::env::temp_dir().join("duphunter-karma-testleri");
        let kaynak = kok.join("kaynak.bin");
        let hedef = kok.join("hedef.bin");
        fs::write(&kaynak, &veri).expect("kaynak yaz");
        let mut k = AkisKuyrugu::yeni(128, 2).expect("kuyruk");
        let kanit = dosya_kopyala_ve_karmala(&kaynak, &hedef, &mut k).expect("kopyala");
        assert_eq!(kanit.boyut, 3000);
        assert_eq!(kanit.blake3, *blake3::hash(&veri).as_bytes());
        assert_eq!(kanit.crc32, crc32(&veri));
        assert_eq!(
            fs::read(&kaynak).expect("kaynak oku"),
            veri,
            "kaynak silinmemeli"
        );
        assert_eq!(fs::read(&hedef).expect("hedef oku"), veri);
    }

    #[test]
    fn kopyalama_bos_dosyada_sifir_kanit_verir() {
        let kok = std::env::temp_dir().join("duphunter-karma-testleri");
        let kaynak = kok.join("bos-kaynak.bin");
        let hedef = kok.join("bos-hedef.bin");
        fs::write(&kaynak, b"").expect("kaynak yaz");
        let mut k = AkisKuyrugu::yeni(64, 2).expect("kuyruk");
        let kanit = dosya_kopyala_ve_karmala(&kaynak, &hedef, &mut k).expect("kopyala");
        assert_eq!(kanit.boyut, 0);
        assert_eq!(kanit.crc32, 0);
        assert_eq!(kanit.blake3, *blake3::hash(b"").as_bytes());
    }
}
