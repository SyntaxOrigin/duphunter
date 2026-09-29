//! Kurtarma arşivi ve geri alma testleri.
//!
//! Bu dosyanın temel iddiası şudur: **DupHunter hiçbir koşulda dosya silmez.**
//! Arşivleme kopyalar, geri alma o kopyaları doğrulayarak geri koyar.

mod yardimci;

use std::fs;
use std::path::PathBuf;

use duphunter::arsiv;
use duphunter::gezgin::IptalBayragi;
use duphunter::grup::kararlar_uret;
use duphunter::hata::Hata;
use duphunter::karma::AkisKuyrugu;
use duphunter::kurallar::Kurallar;
use duphunter::motor;

use yardimci::{sahte_veri, GeciciDizin};

fn kuyruk() -> AkisKuyrugu {
    AkisKuyrugu::yeni(64 * 1024, 2).expect("kuyruk olustur")
}

/// Varsayılan kurallarla, her grupta ilk üyeyi koruyan kararları üretir.
fn kararlar_uret_icin(sonuc: &duphunter::MotorSonucu) -> Vec<duphunter::Karar> {
    kararlar_uret(&sonuc.gruplar, &Kurallar::varsayilan(), &[])
}

/// Verilen kökte üç kopya üretir: `a/x.txt`, `b/x.txt`, `c/x.txt` aynı içerikte.
fn uc_kopya_uret(g: &GeciciDizin) -> Vec<PathBuf> {
    let icerik = sahte_veri(4096, 11);
    vec![
        g.dosya("a/x.txt", &icerik),
        g.dosya("b/x.txt", &icerik),
        g.dosya("c/x.txt", &icerik),
    ]
}

#[test]
fn arsivleme_kaynaklari_yerinde_birakir() {
    let g = GeciciDizin::yeni("arsiv-tasima").expect("gecici dizin");
    let yollar = uc_kopya_uret(&g);
    let mut k = kuyruk();
    let (_tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    assert_eq!(sonuc.gruplar.len(), 1);
    assert_eq!(sonuc.gruplar[0].uyeler.len(), 3);
    let kararlar = kararlar_uret_icin(&sonuc);

    let arsiv_dizini = g.yol().join("kurtarma");
    let arsiv = arsiv::arsiv_olustur(&arsiv_dizini, &kararlar, &mut k).expect("arsiv olustur");

    // Kaynakların üçü de hâlâ yerinde.
    for yol in &yollar {
        assert!(yol.is_file(), "kaynak silinmemeli: {}", yol.display());
        assert_eq!(fs::read(yol).expect("oku").len(), 4096);
    }
    // Arşivde iki kopya var (biri korundu).
    assert_eq!(arsiv.manifest.kayitlar.len(), 2);
    let mut arsiv_toplam = 0u64;
    for kayit in &arsiv.manifest.kayitlar {
        let kopya = arsiv_dizini.join(&kayit.arsivdeki_yol);
        assert!(kopya.is_file(), "arsiv kopyasi eksik: {}", kopya.display());
        assert_eq!(
            fs::read(&kopya).expect("arsiv oku"),
            fs::read(&kayit.kaynak_yol).expect("kaynak oku")
        );
        arsiv_toplam += kayit.boyut;
    }
    assert_eq!(arsiv_toplam, 8192);
}

#[test]
fn arsiv_dogrulamasi_ve_geri_alma_basarili() {
    let g = GeciciDizin::yeni("geri-alma").expect("gecici dizin");
    uc_kopya_uret(&g);
    let mut k = kuyruk();
    let (_tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    let kararlar = kararlar_uret_icin(&sonuc);
    let arsiv_dizini = g.yol().join("kurtarma");
    let arsiv = arsiv::arsiv_olustur(&arsiv_dizini, &kararlar, &mut k).expect("arsiv olustur");

    let ozet = arsiv::dogrula(&arsiv, &mut k).expect("dogrula");
    assert_eq!(ozet.dosya, 2);
    assert_eq!(ozet.bayt, 8192);

    // Geri alma: hedef dizin boş olduğu için başarılı olmalı.
    let hedef = g.yol().join("geri-alindi");
    fs::create_dir_all(&hedef).expect("hedef dizini");
    let yazilan = arsiv::geri_al(&arsiv, Some(&hedef), &mut k).expect("geri al");
    assert_eq!(
        yazilan.len(),
        2,
        "ad cakismasi olsa da ikisi de geri alinmali"
    );
    for yol in &yazilan {
        assert_eq!(fs::read(yol).expect("geri alinan oku").len(), 4096);
    }
    // Üçü de "x.txt" olduğu için geri alınan adlar sıra numarasıyla ayrışır.
    let adlar: Vec<String> = yazilan
        .iter()
        .map(|y| y.file_name().expect("ad").to_string_lossy().to_string())
        .collect();
    assert!(adlar.iter().all(|a| a.starts_with("000")), "{:?}", adlar);
}

#[test]
fn arsiv_geri_alma_var_olan_hedefin_ustune_yazmaz() {
    let g = GeciciDizin::yeni("hedef-var").expect("gecici dizin");
    uc_kopya_uret(&g);
    let mut k = kuyruk();
    let (_tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    let kararlar = kararlar_uret_icin(&sonuc);
    let arsiv_dizini = g.yol().join("kurtarma");
    let arsiv = arsiv::arsiv_olustur(&arsiv_dizini, &kararlar, &mut k).expect("arsiv olustur");
    let hedef = g.yol().join("geri-alindi");
    fs::create_dir_all(&hedef).expect("hedef dizini");
    let _ = arsiv::geri_al(&arsiv, Some(&hedef), &mut k).expect("ilk geri alma");
    let ikinci = arsiv::geri_al(&arsiv, Some(&hedef), &mut k);
    assert!(matches!(ikinci, Err(Hata::HedefVar { .. })), "{:?}", ikinci);
}

#[test]
fn arsiv_bozuk_kopya_geri_almayi_durdurur() {
    let g = GeciciDizin::yeni("bozuk-arsiv").expect("gecici dizin");
    uc_kopya_uret(&g);
    let mut k = kuyruk();
    let (_tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    let kararlar = kararlar_uret_icin(&sonuc);
    let arsiv_dizini = g.yol().join("kurtarma");
    let arsiv = arsiv::arsiv_olustur(&arsiv_dizini, &kararlar, &mut k).expect("arsiv olustur");

    // Arşivdeki bir kopyayı boz: doğrulama yakalamalı ve geri alma yapılmamalı.
    let bozulan = arsiv_dizini.join(&arsiv.manifest.kayitlar[0].arsivdeki_yol);
    fs::write(&bozulan, b"bu icerik degisti").expect("boz");

    let hedef = g.yol().join("geri-alindi");
    fs::create_dir_all(&hedef).expect("hedef dizini");
    let sonuc = arsiv::geri_al(&arsiv, Some(&hedef), &mut k);
    assert!(
        matches!(sonuc, Err(Hata::ArsivDogrulamaBasarisiz { .. })),
        "bozuk arsiv geri alinmamali: {:?}",
        sonuc
    );
    let yazilan = fs::read_dir(&hedef).expect("hedef oku").count();
    assert_eq!(
        yazilan, 0,
        "dogrulama basarisizken hicbir dosya yazilmamali"
    );
}

#[test]
fn bozuk_manifest_acsiz_kalmaz() {
    let g = GeciciDizin::yeni("bozuk-manifest").expect("gecici dizin");
    let arsiv_dizini = g.yol().join("kurtarma");
    fs::create_dir_all(&arsiv_dizini).expect("arsiv dizini");
    fs::write(arsiv_dizini.join(arsiv::MANIFEST_ADI), "{bozuk json").expect("yaz");
    let sonuc = arsiv::arsiv_ac(&arsiv_dizini);
    assert!(matches!(sonuc, Err(Hata::ArsivBozuk { .. })), "{:?}", sonuc);
}

#[test]
fn kismi_manifest_bildirilir_ve_gecerli_sayilmaz() {
    let g = GeciciDizin::yeni("kismi-manifest").expect("gecici dizin");
    let arsiv_dizini = g.yol().join("kurtarma");
    fs::create_dir_all(&arsiv_dizini).expect("arsiv dizini");
    fs::write(arsiv_dizini.join(arsiv::KISMI_ADI), "{}").expect("yaz");
    assert!(arsiv::kismi_manifest(&arsiv_dizini).is_some());
    // Doğrulanmamış arşiv açılamaz.
    let sonuc = arsiv::arsiv_ac(&arsiv_dizini);
    assert!(sonuc.is_err(), "kismi manifest gecerli sayilmamali");
}

#[test]
fn yazma_izni_hatasi_arsivi_yarim_birakmaz() {
    let g = GeciciDizin::yeni("izin-hatasi").expect("gecici dizin");
    let yollar = uc_kopya_uret(&g);
    let mut k = kuyruk();
    let (_tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    let kararlar = kararlar_uret_icin(&sonuc);

    // Kurtarma dizini bir dosyanın altında: dizin oluşturulamaz (izin/yol hatası).
    let engel = g.dosya("engel.txt", b"bu bir dosya");
    let hatali_hedef = engel.join("kurtarma");
    let sonuc = arsiv::arsiv_olustur(&hatali_hedef, &kararlar, &mut k);
    assert!(sonuc.is_err(), "hatali hedef hatasi vermeli");
    for yol in &yollar {
        assert!(yol.is_file(), "hata durumunda kaynak yerinde olmali");
    }
    assert_eq!(
        fs::read(&engel).expect("engel oku"),
        b"bu bir dosya".to_vec()
    );
}

#[test]
fn ad_cakismasi_arsivde_kaybolmaz() {
    let g = GeciciDizin::yeni("arsiv-ad-cakismasi").expect("gecici dizin");
    let yollar = uc_kopya_uret(&g);
    let mut k = kuyruk();
    let (_tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    let kararlar = kararlar_uret_icin(&sonuc);
    let arsiv_dizini = g.yol().join("kurtarma");
    let arsiv = arsiv::arsiv_olustur(&arsiv_dizini, &kararlar, &mut k).expect("arsiv olustur");

    // Üçü de "x.txt": arşivdeki göreli yollar benzersiz olmalı.
    let mut goreli: Vec<String> = arsiv
        .manifest
        .kayitlar
        .iter()
        .map(|k| k.arsivdeki_yol.to_string_lossy().to_string())
        .collect();
    goreli.sort();
    let benzersiz = {
        let mut kopya = goreli.clone();
        kopya.dedup();
        kopya.len() == goreli.len()
    };
    assert!(benzersiz, "arsiv yollari cakismamali: {:?}", goreli);
    for kayit in &arsiv.manifest.kayitlar {
        assert!(
            yollar.contains(&kayit.kaynak_yol),
            "kaynak yol dogru olmali"
        );
    }
}

#[test]
fn tarama_ve_arsivleme_kaynagi_bozmaz() {
    let g = GeciciDizin::yeni("kaynak-bozulmaz").expect("gecici dizin");
    let yollar = uc_kopya_uret(&g);
    let once: Vec<Vec<u8>> = yollar
        .iter()
        .map(|y| fs::read(y).expect("onceki okuma"))
        .collect();
    let mut k = kuyruk();
    let (_tarama, sonuc) = motor::tara_ve_uret(
        g.yol(),
        &Kurallar::varsayilan(),
        &mut k,
        &IptalBayragi::yeni(),
        &mut |_| {},
    )
    .expect("tara");
    let kararlar = kararlar_uret_icin(&sonuc);
    let _ = arsiv::arsiv_olustur(&g.yol().join("kurtarma"), &kararlar, &mut k).expect("arsiv");
    for (yol, veri) in yollar.iter().zip(&once) {
        assert_eq!(&fs::read(yol).expect("sonraki okuma"), veri);
    }
}
