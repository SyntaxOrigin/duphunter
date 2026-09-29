#![forbid(unsafe_code)]
#![warn(clippy::unwrap_used, clippy::expect_used)]

//! DupHunter komut satırı arayüzü.
//!
//! Üç alt komut vardır:
//!
//! - `scan`       — dizini tarar, grupları ekrana basar, isteğe bağlı rapor yazar
//!   ve `--arsivle` verilirse kopyaları kurtarma arşivine taşır.
//! - `hash-report`— tarama yapar ve yalnızca kanıt tablosunu (BLAKE3/CRC32) üretir.
//! - `restore`    — bir kurtarma arşivini doğrular ve dosyaları geri koyar.
//!
//! Hiçbir komut dosya silmez.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use duphunter::arsiv;
use duphunter::gezgin::IptalBayragi;
use duphunter::grup::{kararlar_uret, KararTuru};
use duphunter::hata::{Hata, Sonuc};
use duphunter::karma::AkisKuyrugu;
use duphunter::kurallar::{Kurallar, VARSAYILAN_KURAL_DOSYASI};
use duphunter::motor;
use duphunter::rapor;

/// Akış tabanlı karmalarla birebir kopya avcısı. Hiçbir dosyayı silmez.
#[derive(Debug, Parser)]
#[command(name = "duphunter", version, about, long_about = None)]
struct Cli {
    /// Çalıştırılacak alt komut.
    #[command(subcommand)]
    komut: Komut,
}

/// Alt komutlar.
#[derive(Debug, Subcommand)]
enum Komut {
    /// Dizini tarar, kopya gruplarını listeler ve isteğe bağlı arşiv yazar.
    Scan(ScanArg),
    /// Yalnızca kanıt tablosunu (BLAKE3 + CRC32) üretir.
    HashReport(HashReportArg),
    /// Kurtarma arşivini doğrular ve dosyaları geri koyar.
    Restore(RestoreArg),
}

/// `scan` komutunun ortak seçenekleri.
#[derive(Debug, Args)]
struct OrtakTarama {
    /// Taranacak dizin.
    #[arg(value_name = "KOK")]
    kok: PathBuf,
    /// Kural dosyası (yoksa öntanılan kurallar kullanılır).
    #[arg(long, value_name = "DOSYA")]
    kurallar: Option<PathBuf>,
    /// En küçük dosya boyutu (bayt). Varsayılan: 1.
    #[arg(long, value_name = "BAYT")]
    min_boyut: Option<u64>,
    /// 0 baytlık dosyaları taramaya al.
    #[arg(long)]
    bos_dosyalar: bool,
    /// Nokta ile başlayan/gizli nitelikli dosyaları atla.
    #[arg(long)]
    gomulu_atla: bool,
    /// Geçici/türetilmiş adlı dosyaları atla (varsayılan: açık).
    #[arg(long)]
    turetilmis_atla: bool,
    /// Türetilmiş adlı dosyaları taramaya al.
    #[arg(long, conflicts_with = "turetilmis_atla")]
    turetilmis_al: bool,
    /// Korunacak uzantı (bu uzantıdaki kopyalar arşivlenmez).
    #[arg(long = "koru-uzanti", value_name = "UZANTI")]
    koru_uzantilar: Vec<String>,
    /// 2. aşama ön izleme penceresi (bayt). Varsayılan: 65536.
    #[arg(long, value_name = "BAYT")]
    onizleme_bayt: Option<usize>,
    /// Akış tamponu boyutu (bayt). Varsayılan: 1048576.
    #[arg(long, value_name = "BAYT")]
    tampon_bayt: Option<usize>,
}

/// `scan` komutu.
#[derive(Debug, Args)]
struct ScanArg {
    /// Taranacak ortak seçenekler.
    #[command(flatten)]
    ortak: OrtakTarama,
    /// JSON kanıt raporunun yolu.
    #[arg(long, value_name = "DOSYA")]
    rapor_json: Option<PathBuf>,
    /// Markdown kanıt raporunun yolu.
    #[arg(long, value_name = "DOSYA")]
    rapor_markdown: Option<PathBuf>,
    /// Kurtarma arşivinin yazılacağı dizin; verilirse arşivleme yapılır.
    #[arg(long, value_name = "DIZIN")]
    arsiv: Option<PathBuf>,
    /// Bu yol grup içinde korunacak kopya olarak seçilir (birden fazla verilebilir).
    #[arg(long = "koru", value_name = "YOL")]
    koru: Vec<PathBuf>,
    /// Yalnızca özeti yazdır, grup ayrıntısını basma.
    #[arg(long)]
    ozet: bool,
}

/// `hash-report` komutu.
#[derive(Debug, Args)]
struct HashReportArg {
    /// Taranacak ortak seçenekler.
    #[command(flatten)]
    ortak: OrtakTarama,
    /// JSON kanıt raporunun yolu.
    #[arg(long, value_name = "DOSYA")]
    rapor_json: Option<PathBuf>,
}

/// `restore` komutu.
#[derive(Debug, Args)]
struct RestoreArg {
    /// Kurtarma arşivinin bulunduğu dizin (içinde `manifest.json` vardır).
    #[arg(value_name = "ARSIV")]
    arsiv: PathBuf,
    /// Dosyaları özgün yolları yerine bu dizine, dosya adıyla geri koy.
    #[arg(long, value_name = "DIZIN")]
    hedef: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let sonuc = match cli.komut {
        Komut::Scan(arg) => calistir_scan(arg),
        Komut::HashReport(arg) => calistir_hash_report(arg),
        Komut::Restore(arg) => calistir_restore(arg),
    };
    match sonuc {
        Ok(()) => ExitCode::SUCCESS,
        Err(hata) => {
            eprintln!("hata: {}", hata);
            ExitCode::FAILURE
        }
    }
}

/// Ortak seçeneklerden kural seti üretir.
fn kurallari_olustur(ortak: &OrtakTarama) -> Sonuc<Kurallar> {
    let mut kurallar = match &ortak.kurallar {
        Some(dosya) => Kurallar::yukle(dosya)?,
        None => Kurallar::varsayilan(),
    };
    if let Some(en_kucuk) = ortak.min_boyut {
        kurallar.minimum_boyut = en_kucuk;
    }
    if ortak.bos_dosyalar {
        kurallar.bos_dosyalari_atla = false;
    }
    if ortak.gomulu_atla {
        kurallar.gomulu_dosyalari_atla = true;
    }
    if ortak.turetilmis_al {
        kurallar.turetilmis_adi_atla = false;
    }
    if ortak.turetilmis_atla {
        kurallar.turetilmis_adi_atla = true;
    }
    for uzanti in &ortak.koru_uzantilar {
        kurallar.korunacak_uzantilar.insert(uzanti.to_lowercase());
    }
    if let Some(pencere) = ortak.onizleme_bayt {
        kurallar.onizleme_bayt = pencere;
    }
    if let Some(tampon) = ortak.tampon_bayt {
        kurallar.tampon_bayt = tampon;
    }
    kurallar.dogrula(PathBuf::from(VARSAYILAN_KURAL_DOSYASI).as_path())?;
    Ok(kurallar)
}

/// Akış kuyruğunu kurallardaki tampon boyutuyla üretir.
fn kuyruk_olustur(kurallar: &Kurallar) -> Sonuc<AkisKuyrugu> {
    AkisKuyrugu::yeni(kurallar.tampon_bayt, 2)
}

fn calistir_scan(arg: ScanArg) -> Sonuc<()> {
    let kurallar = kurallari_olustur(&arg.ortak)?;
    let mut kuyruk = kuyruk_olustur(&kurallar)?;
    let iptal = IptalBayragi::yeni();
    let mut ilerleme = |durum: &motor::Ilerleme| {
        if durum.toplam > 0 && durum.islenen % 256 == 0 {
            eprintln!("  {:?}: {}/{}", durum.asama, durum.islenen, durum.toplam);
        }
    };
    let (tarama, motor_sonucu) = motor::tara_ve_uret(
        &arg.ortak.kok,
        &kurallar,
        &mut kuyruk,
        &iptal,
        &mut ilerleme,
    )?;
    let kararlar = kararlar_uret(&motor_sonucu.gruplar, &kurallar, &arg.koru);

    if !arg.ozet {
        grup_tablosunu_bas(&motor_sonucu, &kararlar);
    }
    ozeti_bas(&tarama, &motor_sonucu, &kurallar, &kararlar);

    let rapor = rapor::rapor_uret(&tarama, &motor_sonucu, &kurallar, &kararlar);
    if let Some(dosya) = &arg.rapor_json {
        rapor::json_yaz(&rapor, dosya)?;
        println!("JSON rapor yazıldı: {}", dosya.display());
    }
    if let Some(dosya) = &arg.rapor_markdown {
        rapor::markdown_yaz(&rapor, dosya)?;
        println!("Markdown rapor yazıldı: {}", dosya.display());
    }

    if let Some(dizin) = &arg.arsiv {
        if motor_sonucu.iptal_edildi {
            return Err(Hata::Parametre {
                ad: "arsiv",
                ayrinti: "tarama iptal edildiği için arşiv yazılmadı".to_string(),
            });
        }
        let arsiv = arsiv::arsiv_olustur(dizin, &kararlar, &mut kuyruk)?;
        arsiv::dogrula(&arsiv, &mut kuyruk)?;
        println!(
            "Kurtarma arşivi doğrulandı: {} ({} dosya)",
            dizin.display(),
            arsiv.manifest.kayitlar.len()
        );
        println!("Hiçbir kaynak dosya silinmedi; kopyalar arşivde bekliyor.");
    }
    Ok(())
}

fn calistir_hash_report(arg: HashReportArg) -> Sonuc<()> {
    let kurallar = kurallari_olustur(&arg.ortak)?;
    let mut kuyruk = kuyruk_olustur(&kurallar)?;
    let iptal = IptalBayragi::yeni();
    let (tarama, motor_sonucu) =
        motor::tara_ve_uret(&arg.ortak.kok, &kurallar, &mut kuyruk, &iptal, &mut |_| {})?;
    println!("# DupHunter kanıt raporu");
    println!("kok: {}", tarama.kok.display());
    println!("taranan dosya: {}", tarama.dosyalar.len());
    for grup in &motor_sonucu.gruplar {
        println!(
            "grup {} | bayt {} | kopya {} | blake3 {}",
            grup.kimlik,
            grup.boyut,
            grup.uyeler.len(),
            duphunter::karma::karma_hex(&grup.uyeler[0].tam_karma)
        );
        for uye in &grup.uyeler {
            println!(
                "    crc32 {:08x} | {:>10} bayt | {}",
                uye.crc32,
                uye.boyut,
                uye.yol.display()
            );
        }
    }
    if let Some(dosya) = &arg.rapor_json {
        let kararlar = kararlar_uret(&motor_sonucu.gruplar, &kurallar, &[]);
        let rapor = rapor::rapor_uret(&tarama, &motor_sonucu, &kurallar, &kararlar);
        rapor::json_yaz(&rapor, dosya)?;
        println!("JSON rapor yazıldı: {}", dosya.display());
    }
    Ok(())
}

fn calistir_restore(arg: RestoreArg) -> Sonuc<()> {
    let arsiv = arsiv::arsiv_ac(&arg.arsiv)?;
    if let Some(kismi) = arsiv::kismi_manifest(&arg.arsiv) {
        eprintln!(
            "uyarı: yarım kalmış manifest bulundu ({}); bu arşiv doğrulanmamıştır",
            kismi.display()
        );
    }
    let mut kuyruk = arsiv::varsayilan_kuyruk()?;
    let yazilan = arsiv::geri_al(&arsiv, arg.hedef.as_deref(), &mut kuyruk)?;
    println!(
        "Doğrulandı ve geri alındı: {} dosya ({} bayt)",
        arsiv.manifest.kayitlar.len(),
        arsiv.manifest.kayitlar.iter().map(|k| k.boyut).sum::<u64>()
    );
    for yol in &yazilan {
        println!("  geri alındı: {}", yol.display());
    }
    Ok(())
}

/// Grup ekranını basar: grup başına tek satır ve üye listesi.
fn grup_tablosunu_bas(sonuc: &motor::MotorSonucu, kararlar: &[duphunter::grup::Karar]) {
    if sonuc.gruplar.is_empty() {
        println!("Birebir kopya bulunamadı.");
        return;
    }
    println!(
        "{:>4}  {:>12}  {:>5}  {:>12}  BLAKE3",
        "GRUP", "BOYUT", "ADET", "KAZANÇ"
    );
    for grup in &sonuc.gruplar {
        println!(
            "{:>4}  {:>12}  {:>5}  {:>12}  {}",
            grup.kimlik,
            rapor::bayt_goster(grup.boyut),
            grup.uyeler.len(),
            rapor::bayt_goster(grup.kazanc_bayt()),
            duphunter::karma::karma_hex(&grup.uyeler[0].tam_karma)
        );
        for uye in &grup.uyeler {
            let karar = kararlar
                .iter()
                .find(|k| k.yol == uye.yol)
                .map(|k| k.tur)
                .unwrap_or(KararTuru::Asla);
            println!(
                "      [{}] {} ({} bayt, crc32 {:08x})",
                match karar {
                    KararTuru::Koru => "koru",
                    KararTuru::Arsivle => "arşiv",
                    KararTuru::Asla => "asla",
                },
                uye.yol.display(),
                uye.boyut,
                uye.crc32
            );
        }
    }
}

/// Tarama özetini basar.
fn ozeti_bas(
    tarama: &duphunter::Tarama,
    sonuc: &motor::MotorSonucu,
    kurallar: &Kurallar,
    kararlar: &[duphunter::grup::Karar],
) {
    let kazanc: u64 = sonuc.gruplar.iter().map(|g| g.kazanc_bayt()).sum();
    let arsiv_adayi = kararlar
        .iter()
        .filter(|k| k.tur == KararTuru::Arsivle)
        .count();
    println!();
    println!("Kök            : {}", tarama.kok.display());
    println!(
        "Dosya          : {} taranan, {} dışlanan, {} okunamayan",
        tarama.dosyalar.len(),
        tarama.atlananlar.len(),
        tarama.hatalar.len()
    );
    println!(
        "Grup           : {} ({} kopya dosya)",
        sonuc.gruplar.len(),
        sonuc.gruplar.iter().map(|g| g.uyeler.len()).sum::<usize>()
    );
    println!(
        "Kazanç         : {} ({} bayt)",
        rapor::bayt_goster(kazanc),
        kazanc
    );
    println!("Arşiv adayı    : {} dosya", arsiv_adayi);
    println!(
        "Okunan bayt    : {} (toplam {} bayt, aşama1 dosya {}, aşama2 {}, aşama3 {})",
        rapor::bayt_goster(sonuc.sayac.toplam_bayt()),
        tarama.toplam_boyut(),
        sonuc.sayac.asama1_dosya,
        sonuc.sayac.asama2_dosya,
        sonuc.sayac.asama3_dosya
    );
    println!(
        "Ayarlar        : min {} bayt, onizleme {} bayt, tampon {} bayt",
        kurallar.minimum_boyut, kurallar.onizleme_bayt, kurallar.tampon_bayt
    );
    if sonuc.iptal_edildi {
        println!("Durum          : İPTAL EDİLDİ — sonuç eksik");
    }
    println!("Güvenlik       : hiçbir dosya silinmez; arşivleme kopyalar ve geri alınabilir.");
}
