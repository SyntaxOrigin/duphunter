//! Uçtan uca tarama testleri: kopyalar, kurallar, kenar durumları ve raporlar.

mod yardimci;

use std::fs;
use std::path::PathBuf;

use duphunter::gezgin::IptalBayragi;
use duphunter::grup::kararlar_uret;
use duphunter::hata::Hata;
use duphunter::karma::AkisKuyrugu;
use duphunter::kurallar::Kurallar;
use duphunter::motor::{self, Asama};
use duphunter::rapor;

use yardimci::{sahte_veri, GeciciDizin};

fn kuyruk() -> AkisKuyrugu {
    AkisKuyrugu::yeni(64 * 1024, 2).expect("kuyruk olustur")
}

#[test]
fn ayni_icerik_farkli_adlar_kopya_grubuna_girir() {
    let g = GeciciDizin::yeni("farkli-ad").expect("gecici dizin");
    let veri = sahte_veri(4096, 1);
    let a = g.dosya("belge/orijinal.txt", &veri);
    let b = g.dosya("yedek/orijinal (kopya).txt", &veri);
    let mut k = kuyruk();
    let (tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert_eq!(tarama.dosyalar.len(), 2);
    assert_eq!(sonuc.gruplar.len(), 1, "ayni icerik tek grup olmali");
    let yollar: Vec<PathBuf> = sonuc.gruplar[0]
        .uyeler
        .iter()
        .map(|u| u.yol.clone())
        .collect();
    assert!(yollar.contains(&a) && yollar.contains(&b));
    assert_eq!(sonuc.gruplar[0].kazanc_bayt(), 4096);
}

#[test]
fn farkli_icerik_ayni_boyut_kopya_sayilmaz() {
    let g = GeciciDizin::yeni("ayni-boyut").expect("gecici dizin");
    g.dosya("a.bin", &sahte_veri(2048, 1));
    g.dosya("b.bin", &sahte_veri(2048, 2));
    let mut k = kuyruk();
    let (_, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert!(sonuc.gruplar.is_empty(), "farkli icerik ayni grup olmamali");
}

#[test]
fn bos_dosya_ve_sifir_bayt_ayni_kurala_tabi() {
    let g = GeciciDizin::yeni("bos-dosya").expect("gecici dizin");
    g.dosya("bos.txt", b"");
    g.dosya("dolu.txt", b"x");
    let mut k = kuyruk();
    let (tarama, _) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert_eq!(tarama.dosyalar.len(), 1, "0 baytlik dosya aday olmamali");
    assert_eq!(tarama.atlananlar.len(), 1);
    assert_eq!(tarama.atlananlar[0].gerekce, "boş dosya");
}

#[test]
fn sifir_bayt_dosyalari_bos_dosyalar_kapaliyken_aday_olur() {
    let g = GeciciDizin::yeni("sifir-bayt-acik").expect("gecici dizin");
    g.dosya("a.txt", b"");
    g.dosya("b.txt", b"");
    let mut kurallar = Kurallar::varsayilan();
    // Boş dosya kuralı kapalı olsa da minimum boyut kuralı 0 baytı dışlar;
    // ikisi birlikte kapatılmalıdır.
    kurallar.bos_dosyalari_atla = false;
    kurallar.minimum_boyut = 0;
    let mut k = kuyruk();
    let (tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &kurallar,
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert_eq!(tarama.dosyalar.len(), 2);
    assert_eq!(sonuc.gruplar.len(), 1, "iki bos dosya birbirinin kopyasi");
    assert_eq!(sonuc.gruplar[0].kazanc_bayt(), 0);
}

#[test]
fn buyuk_dosya_akis_kuyrugu_sinirini_asmaz() {
    let g = GeciciDizin::yeni("buyuk-dosya").expect("gecici dizin");
    let veri = sahte_veri(8 * 1024 * 1024, 7);
    let dosya = g.dosya("buyuk.bin", &veri);
    let mut k = AkisKuyrugu::yeni(64 * 1024, 2).expect("kuyruk");
    let ayrilan = k.ayrilan_bellek();
    let karma = duphunter::karma::tam_karma(&dosya, &mut k).expect("tam karma");
    assert_eq!(karma, *blake3::hash(&veri).as_bytes());
    assert_eq!(
        k.ayrilan_bellek(),
        ayrilan,
        "bellek dosya boyutunca artmamali"
    );
    assert_eq!(k.gecen_bayt(), 8 * 1024 * 1024);
    let onizleme =
        duphunter::karma::onizleme_hesapla(&dosya, 64 * 1024, &mut kuyruk()).expect("onizleme");
    assert_eq!(
        onizleme.okunan_bayt,
        2 * 64 * 1024,
        "onizleme yalnizca bas ve son pencereyi okumali"
    );
}

#[test]
fn crc_cakismasi_tam_karma_ile_ayrilir() {
    let g = GeciciDizin::yeni("crc-cakismasi").expect("gecici dizin");
    // Aynı boyut, aynı baş/son penceresi, farklı orta: ön eleme çakışır,
    // tam akış BLAKE3 ayırır.
    let bas = sahte_veri(1024, 3);
    let son = sahte_veri(1024, 4);
    let orta_a = vec![0xAAu8; 1024];
    let orta_b = vec![0xBBu8; 1024];
    let mut a = bas.clone();
    a.extend_from_slice(&orta_a);
    a.extend_from_slice(&son);
    let mut b = bas.clone();
    b.extend_from_slice(&orta_b);
    b.extend_from_slice(&son);
    g.dosya("a.bin", &a);
    g.dosya("b.bin", &b);
    assert_eq!(a.len(), b.len());

    let mut kurallar = Kurallar::varsayilan();
    kurallar.onizleme_bayt = 1024;
    let mut k = kuyruk();
    let (_, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &kurallar,
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert_eq!(sonuc.sayac.asama2_dosya, 2);
    assert_eq!(sonuc.sayac.asama3_dosya, 2, "cakisma 3. asamaya tasinmali");
    assert!(
        sonuc.gruplar.is_empty(),
        "cakisma karmasi kopya sayilmamali"
    );
}

#[test]
fn turetilmis_dosya_adlari_tespit_edilir() {
    let g = GeciciDizin::yeni("turetilmis").expect("gecici dizin");
    let icerik = sahte_veri(512, 9);
    g.dosya("gercek/belge.txt", &icerik);
    g.dosya("gercek/~$belge.docx", &icerik);
    g.dosya("gercek/gecici.tmp", &icerik);
    let mut k = kuyruk();
    let (tarama, _) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    let adlar: Vec<String> = tarama
        .dosyalar
        .iter()
        .map(|d| d.yol.file_name().expect("ad").to_string_lossy().to_string())
        .collect();
    assert!(adlar.contains(&"belge.txt".to_string()));
    assert!(!adlar.contains(&"~$belge.docx".to_string()));
    assert!(!adlar.contains(&"gecici.tmp".to_string()));
    let gerekce = tarama
        .atlananlar
        .iter()
        .find(|a| a.yol.to_string_lossy().contains("gecici.tmp"))
        .map(|a| a.gerekce.clone())
        .unwrap_or_default();
    assert!(gerekce.contains("türetilmiş"), "{}", gerekce);
}

#[test]
fn gomulu_dosya_kurali_acik_kapali_denenir() {
    let g = GeciciDizin::yeni("gomulu").expect("gecici dizin");
    let icerik = sahte_veri(256, 2);
    g.dosya(".gizli.txt", &icerik);
    g.dosya("gorunur.txt", &icerik);

    let mut acik = Kurallar::varsayilan();
    acik.gomulu_dosyalari_atla = false;
    let mut k = kuyruk();
    let (tarama_acik, sonuc_acik) =
        motor::tara_ve_uret(g.yol(), &acik, &mut k, &IptalBayragi::yeni(), &mut |_| {})
            .expect("tara");
    assert_eq!(
        tarama_acik.dosyalar.len(),
        2,
        "gizli kurali kapaliyken iki dosya aday"
    );
    assert_eq!(sonuc_acik.gruplar.len(), 1);

    let mut kapali = Kurallar::varsayilan();
    kapali.gomulu_dosyalari_atla = true;
    let mut k2 = kuyruk();
    let (tarama_kapali, sonuc_kapali) = motor::tara_ve_uret(
        g.yol(),
        &kapali,
        &mut k2,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert_eq!(
        tarama_kapali.dosyalar.len(),
        1,
        "gizli kurali acikken nokta basli dosya atlanmali"
    );
    assert_eq!(sonuc_kapali.gruplar.len(), 0);
}

#[test]
fn korunacak_uzanti_kurali_acik_kapali_denenir() {
    let g = GeciciDizin::yeni("korunacak").expect("gecici dizin");
    let icerik = sahte_veri(1000, 5);
    g.dosya("a.txt", &icerik);
    g.dosya("b.keep", &icerik);
    let mut kurallar = Kurallar::varsayilan();
    kurallar.korunacak_uzantilar.insert("keep".to_string());
    let mut k = kuyruk();
    let (tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &kurallar,
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert_eq!(tarama.dosyalar.len(), 2);
    let kararlar = kararlar_uret(&sonuc.gruplar, &kurallar, &[]);
    let asla = kararlar
        .iter()
        .filter(|k| k.tur == duphunter::grup::KararTuru::Asla)
        .count();
    assert_eq!(asla, 1, "korunacak uzantili kopya arsivlenmemeli");
}

#[test]
fn cok_derin_dizin_taranir() {
    let g = GeciciDizin::yeni("derin").expect("gecici dizin");
    let mut yol = String::new();
    for i in 0..40 {
        yol.push_str(&format!("seviye{:02}/", i));
    }
    let icerik = sahte_veri(700, 6);
    g.dosya(&format!("{}{}.txt", yol, "derin"), &icerik);
    let mut k = kuyruk();
    let (tarama, _) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert_eq!(tarama.dosyalar.len(), 1, "cok derin dosya bulunmali");
    assert!(tarama.dizin_sayisi >= 40);
}

#[test]
fn windows_uzun_yolu_islenir_veya_hata_dondurur() {
    let g = GeciciDizin::yeni("uzun-yol").expect("gecici dizin");
    let mut yol = String::new();
    for i in 0..7 {
        yol.push_str(&format!("klasor-{:02}-", i));
    }
    yol.push_str("cok-uzun-dosya-adi.txt");
    let icerik = sahte_veri(333, 8);
    let dosya = g.dosya(&yol, &icerik);
    let mut k = kuyruk();
    match duphunter::karma::tam_karma(&dosya, &mut k) {
        Ok(karma) => assert_eq!(karma, *blake3::hash(&icerik).as_bytes()),
        Err(hata) => {
            // Uzun yol açılamıyorsa bu bir panik değil, düzgün bir hatadır.
            assert!(matches!(hata, Hata::Io { .. }), "{:?}", hata);
        }
    }
}

#[test]
fn hatali_yol_hatasi_dondurur() {
    let mut k = kuyruk();
    let sonuc = motor::tara_ve_uret(
        std::path::Path::new("C:/boyle-bir-dizin-08-yok"),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    );
    assert!(matches!(sonuc, Err(Hata::Io { .. })), "{:?}", sonuc);
}

#[test]
fn kismi_tarama_iptali_yarim_sonuc_dondurur() {
    let g = GeciciDizin::yeni("iptal").expect("gecici dizin");
    // Aynı boyutlu çiftler: 1. aşama aday üretsin, 2. aşama çalışsın.
    for i in 0..20u32 {
        let icerik = sahte_veri(1000, i as u8);
        g.dosya(&format!("a{:02}.bin", i), &icerik);
        g.dosya(&format!("b{:02}.bin", i), &icerik);
    }
    let iptal = IptalBayragi::yeni();
    let kopya = iptal.clone();
    let mut k = kuyruk();
    let (tarama, sonuc) =
        motor::tara_ve_uret(g.yol(), &Kurallar::varsayilan(), &mut k, &iptal, &mut |d| {
            if d.asama == Asama::Onizleme && d.islenen == 0 {
                kopya.iptal_et();
            }
        })
        .expect("tara");
    assert!(
        sonuc.iptal_edildi || tarama.iptal_edildi,
        "iptal isareti bekleniyor"
    );
}

#[test]
fn ad_cakismasi_olan_dosyalar_ayri_kaydedilir() {
    let g = GeciciDizin::yeni("ad-cakismasi").expect("gecici dizin");
    let icerik = sahte_veri(300, 4);
    g.dosya("klasor-b/dosya.txt", &icerik);
    g.dosya("klasor-a/dosya.txt", &icerik);
    let mut k = kuyruk();
    let (_, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert_eq!(sonuc.gruplar.len(), 1);
    assert_eq!(sonuc.gruplar[0].uyeler.len(), 2);
    let adlar: Vec<String> = sonuc.gruplar[0]
        .uyeler
        .iter()
        .map(|u| u.yol.file_name().expect("ad").to_string_lossy().to_string())
        .collect();
    assert_eq!(
        adlar,
        vec!["dosya.txt".to_string(), "dosya.txt".to_string()]
    );
}

#[test]
fn tarama_kurallari_uygular_ve_raporu_disa_yazar() {
    let g = GeciciDizin::yeni("rapor").expect("gecici dizin");
    let icerik = sahte_veri(2048, 1);
    g.dosya("bir/rapor.txt", &icerik);
    g.dosya("iki/rapor-yedek.txt", &icerik);
    g.dosya("iki/rapor.tmp", &icerik);

    let mut k = kuyruk();
    let (tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    let kurallar = Kurallar::varsayilan();
    let kararlar = kararlar_uret(&sonuc.gruplar, &kurallar, &[]);
    let rapor = rapor::rapor_uret(&tarama, &sonuc, &kurallar, &kararlar);

    let json_yol = g.yol().join("cikti/rapor.json");
    let md_yol = g.yol().join("cikti/rapor.md");
    rapor::json_yaz(&rapor, &json_yol).expect("json yaz");
    rapor::markdown_yaz(&rapor, &md_yol).expect("markdown yaz");

    let ham = fs::read_to_string(&json_yol).expect("json oku");
    let geri: rapor::TaramaRaporu = serde_json::from_str(&ham).expect("json ayristir");
    assert_eq!(geri.ozet.grup, 1);
    assert_eq!(geri.ozet.kazanc_bayt, 2048);
    assert_eq!(geri.ozet.arsiv_adayi, 1);
    assert_eq!(geri.ozet.korunan, 1);
    assert_eq!(geri.gruplar[0].uyeler.len(), 2);
    for uye in &geri.gruplar[0].uyeler {
        assert_eq!(uye.blake3.len(), 64, "onaltilik karma 64 karakter olmali");
        assert_eq!(uye.crc32.len(), 8);
    }

    let md = fs::read_to_string(&md_yol).expect("markdown oku");
    assert!(md.contains("## Özet"));
    assert!(md.contains("## Kademeli aday üretimi"));
    assert!(md.contains("hiçbir dosyayı silmez"));
}

#[test]
fn kademeli_aday_uretimi_tam_okumayi_engeller() {
    let g = GeciciDizin::yeni("tasarruf").expect("gecici dizin");
    // 40 benzersiz ve 256 KiB'lık dosya: hepsi aynı boyutta oldukları için
    // 1. aşamadan geçerler, 2. aşamada elenirler; 3. aşama hiç çalışmaz.
    for i in 0..40u32 {
        g.dosya(
            &format!("tekil-{:03}.bin", i),
            &sahte_veri(256 * 1024, i as u8),
        );
    }
    let mut k = kuyruk();
    let (tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert_eq!(tarama.dosyalar.len(), 40);
    assert!(sonuc.gruplar.is_empty());
    let toplam = tarama.toplam_boyut();
    assert_eq!(sonuc.sayac.asama2_dosya, 40);
    assert_eq!(
        sonuc.sayac.asama3_dosya, 0,
        "onizlemede elenen dosya tam okunmamali"
    );
    assert_eq!(sonuc.sayac.asama3_bayt, 0);
    assert!(
        sonuc.sayac.toplam_bayt() * 2 <= toplam,
        "2. asama dosya basi ve sonu kadar okumali (en fazla yarisi): {} * 2 <= {}",
        sonuc.sayac.toplam_bayt(),
        toplam
    );
}
