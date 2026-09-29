# DupHunter (KopyaAvcı)

Akış tabanlı karmalarla **birebir kopya avcısı**. Üç aşamalı aday üretimiyle
gereksiz tam okumayı önler, "bu iki dosya aynı" iddiasını kriptografik bir
kanıtla (BLAKE3) destekler ve — en önemlisi — **hiçbir koşulda dosya silmez**:
karar verilen kopyalar kurtarma arşivine kopyalanır, kaynak yerinde bırakılır
ve `restore` ile geri alınabilir.

Fikir raporu: `%USERPROFILE%\Desktop\Fikirler\08-kopya-avcisi.html`

---

## Özellikler

- **Üç aşamalı aday üretimi.** (1) Boyuta göre gruplama — hiç okuma yapılmaz;
  (2) dosyanın **baş ve son penceresinden** hızlı ön eleme (CRC32 + baş/son
  BLAKE3); (3) yalnızca 2. aşamada eşleşen dosyalar için tam akış BLAKE3.
  "Birebir kopya" iddiası yalnızca 3. aşamanın kanıtıdır; CRC32 tek başına
  asla eşleşme kanıtı sayılmaz.
- **Sabit bellekli akış kuyruğu.** Okuma, bir kez ayrılan ve yeniden kullanılan
  dilimler üzerinden yapılır. 1 MiB varsayılan tamponla 4 GB'lık bir dosya
  taranırken bile ayrılan bellek değişmez; hiçbir dosya belleğe tamamen
  alınmaz. Boyutu önceden bilinmeyen akışlarda da çalışır.
- **Kullanıcı kuralları (düz metin JSON).** Minimum boyut, boş dosya, gömülü
  dosya, türetilmiş ad (`.tmp`, `~$`, `~`, `.bak`, `.part` …), korunacak
  uzantılar ve hariç tutulacak yol joker desenleri.
- **Kurtarma arşivi — silme değil, taşıma.** Karar verilen dosyalar seçilen
  dizine kopyalanır; `manifest.json` kaynak yolu, arşiv içi yol, boyut, CRC32,
  BLAKE3 ve zaman damgasını saklar. Arşiv yazıldıktan sonra **geri okuma
  testi** yapılır; doğrulama başarısızsa manifest `manifest.json.partial`
  adıyla kalır ve arşiv geçerli sayılmaz.
- **Geri alma.** `restore` arşivi doğrular ve dosyaları özgün yollarına ya da
  istenirse ayrı bir dizine geri koyar. Mevcut bir hedefin üzerine yazmaz.
- **Kanıt raporu (JSON + Markdown).** Grup başına ortak BLAKE3, dosya başına
  CRC32, aşama bazlı okunan bayt sayacı, uygulanan kurallar, dışlanan
  dosyaların gerekçesi ve okunamayan girişlerin listesi.
- **Üç alt komut:** `scan`, `hash-report`, `restore`. Sıfır ağ trafiği, sıfır
  gizli veri toplama.

## Kurulum

Gereksinim: Rust 1.74 veya üzeri (MSRV beyanı `rust-version = "1.74"`).
Bu depoda **yalnızca** `cargo 1.98.1` / `rustc 1.98.1` ile derlenip
doğrulandı; daha eski bir araç zinciriyle gerçek MSRV sınaması yapılmadı.

```console
$ cargo build --release
   Compiling blake3 v1.8.7
   Compiling duphunter v0.1.0 (%USERPROFILE%\Desktop\Projeler\projects\08-duphunter)
    Finished `release` profile [optimized] target(s) in 19.03s
```

Çalıştırmak için iki seçenek var:

```console
$ cargo install --path .
  Installing %USERPROFILE%\.cargo\bin\duphunter.exe
  Installed package `duphunter v0.1.0 (%USERPROFILE%\Desktop\Projeler\projects\08-duphunter)` (executable `duphunter.exe`)
$ duphunter --version
duphunter 0.1.0
```

İkinci komut ikiliyi `~/.cargo/bin` altına kopyalar ve `duphunter` adıyla
PATH'e ekler. Tek bir yürütülebilir dosya üretilir; harici kütüphane,
kurulum adımı veya çalışma zamanı bağımlılığı yoktur (blake3 ve crc32fast
ikiliye gömülü derlenir).

## Kullanım

Aşağıdaki çıktıların tamamı gerçek bir demo veri kümesi üzerinde üretilmiştir.
Veri kümesi şöyle hazırlandı:

```console
$ demo = "%USERPROFILE%\AppData\Local\Temp\duphunter-demo"
$ icerik = "Toplanti tutanaklari ve mali rapor 2025 - ikinci ceyrek" * 40
# Belgeler\2024\tutanak.txt, Belgeler\2025\tutanak (kopya).txt,
# Belgeler\2025\tutanak-yedek.txt  -> ayni icerik, 2200 bayt
# Belgeler\2024\butce.txt, butce2.txt -> ayni boyut (1024), farkli icerik
# Projeler\notlar.tmp -> turetilmis ad
# Projeler\.git\objects\abc -> haric yol deseni
```

### 1. Yardım çıktısı

```console
$ duphunter.exe --help
Akis tabanli karmalarla birebir kopya avci; hicbir dosyayi silmez, yalnizca kurtarma arsivine tasiyan kanit ureten terminal araci.

Usage: duphunter.exe <COMMAND>

Commands:
  scan         Dizini tarar, kopya gruplarını listeler ve isteğe bağlı arşiv yazar
  hash-report  Yalnızca kanıt tablosunu (BLAKE3 + CRC32) üretir
  restore      Kurtarma arşivini doğrular ve dosyaları geri koyar
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### 2. Tarama ve grup ekranı

```console
$ duphunter.exe scan %USERPROFILE%\AppData\Local\Temp\duphunter-demo
  Onizleme: 0/5
  TamKarma: 0/3
GRUP         BOYUT   ADET        KAZANÇ  BLAKE3
   0      2.15 KiB      3      4.30 KiB  62ca3382d5e6aa92108e0da625d1a29f987e6f5c342a82ade835c179e5ad122f
      [koru] ...\Belgeler\2024\tutanak.txt (2200 bayt, crc32 c883a7e2)
      [arşiv] ...\Belgeler\2025\tutanak (kopya).txt (2200 bayt, crc32 c883a7e2)
      [arşiv] ...\Belgeler\2025\tutanak-yedek.txt (2200 bayt, crc32 c883a7e2)

Kök            : %USERPROFILE%\AppData\Local\Temp\duphunter-demo
Dosya          : 6 taranan, 2 dışlanan, 0 okunamayan
Grup           : 1 (3 kopya dosya)
Kazanç         : 4.30 KiB (4400 bayt)
Arşiv adayı    : 2 dosya
Okunan bayt    : 14.89 KiB (toplam 8670 bayt, aşama1 dosya 6, aşama2 5, aşama3 3)
Ayarlar        : min 1 bayt, onizleme 65536 bayt, tampon 1048576 bayt
Güvenlik       : hiçbir dosya silinmez; arşivleme kopyalar ve geri alınabilir.
```

Aynı boyutlu ama farklı içerikli `butce.txt` / `butce2.txt` **gruba girmedi**:
1. aşamada eşleştiler, 2. aşamadaki CRC32 ve baş/son BLAKE3 onları eledi.

### 3. Kanıt tablosu (`hash-report`)

```console
$ duphunter.exe hash-report %USERPROFILE%\AppData\Local\Temp\duphunter-demo
# DupHunter kanıt raporu
kok: %USERPROFILE%\AppData\Local\Temp\duphunter-demo
taranan dosya: 6
grup 0 | bayt 2200 | kopya 3 | blake3 62ca3382d5e6aa92108e0da625d1a29f987e6f5c342a82ade835c179e5ad122f
    crc32 c883a7e2 |       2200 bayt | ...\Belgeler\2024\tutanak.txt
    crc32 c883a7e2 |       2200 bayt | ...\Belgeler\2025\tutanak (kopya).txt
    crc32 c883a7e2 |       2200 bayt | ...\Belgeler\2025\tutanak-yedek.txt
```

### 4. Arşivleme ve rapor yazımı

```console
$ duphunter.exe scan C:\...\duphunter-demo --arsiv C:\...\duphunter-kurtarma `
      --rapor-json C:\...\duphunter-rapor.json `
      --rapor-markdown C:\...\duphunter-rapor.md --ozet
Kök            : %USERPROFILE%\AppData\Local\Temp\duphunter-demo
Dosya          : 6 taranan, 2 dışlanan, 0 okunamayan
Grup           : 1 (3 kopya dosya)
Kazanç         : 4.30 KiB (4400 bayt)
Arşiv adayı    : 2 dosya
Okunan bayt    : 14.89 KiB (toplam 8670 bayt, aşama1 dosya 6, aşama2 5, aşama3 3)
JSON rapor yazıldı: C:\...\duphunter-rapor.json
Markdown rapor yazıldı: C:\...\duphunter-rapor.md
Kurtarma arşivi doğrulandı: C:\...\duphunter-kurtarma (2 dosya)
Hiçbir kaynak dosya silinmedi; kopyalar arşivde bekliyor.
```

Arşiv içeriği ve `manifest.json`:

```console
$ Get-ChildItem -Recurse C:\...\duphunter-kurtarma | Select-Object -ExpandProperty Name
dosyalar
manifest.json
00000_tutanak (kopya).txt
00001_tutanak-yedek.txt
```

```json
{
  "surum": 1,
  "olusturma_sn": 1790648218,
  "kayitlar": [
    {
      "kaynak_yol": "C:\\...\\Belgeler\\2025\\tutanak (kopya).txt",
      "arsivdeki_yol": "dosyalar\\00000_tutanak (kopya).txt",
      "boyut": 2200,
      "crc32": 1186139838,
      "blake3": "62ca3382d5e6aa92108e0da625d1a29f987e6f5c342a82ade835c179e5ad122f",
      "zaman_sn": 1790648218
    }
  ]
}
```

**Kaynak dosyaların üçü de arşivden sonra yerinde duruyor.** Komutu
`Get-ChildItem -Recurse -File` ile saydığınızda demo klasöründe hâlâ 8 dosya
vardır.

### 5. Geri alma

```console
$ duphunter.exe restore C:\...\duphunter-kurtarma --hedef C:\...\duphunter-geri
Doğrulandı ve geri alındı: 2 dosya (4400 bayt)
  geri alındı: C:\...\duphunter-geri\00000_tutanak (kopya).txt
  geri alındı: C:\...\duphunter-geri\00001_tutanak-yedek.txt
```

Aynı komut ikinci kez çalıştırılırsa var olan hedefin üzerine yazmaz:

```console
$ duphunter.exe restore C:\...\duphunter-kurtarma --hedef C:\...\duphunter-geri
hata: geri alma hedefi zaten var, üzerine yazılmadı: C:\...\duphunter-geri\00000_tutanak (kopya).txt
$ echo $LASTEXITCODE
1
```

## Test

```console
$ cargo test
   Compiling duphunter v0.1.0 (%USERPROFILE%\Desktop\Projeler\projects\08-duphunter)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 4.20s
     Running unittests src\lib.rs (target\debug\deps\duphunter-da34fa7ecab45835.exe)
running 92 tests
test result: ok. 92 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests\arsiv_geri_alma.rs
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests\silme_olmaz_kaniti.rs
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests\tarama_butunlesme.rs
running 16 tests
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   Doc-tests duphunter
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**test sonucu: okunan 121; geçen 121; başarısız 0** (92 birim + 29 entegrasyon).

Kapsanan kenar durumları:

| Senaryo | Nerede |
|---|---|
| Aynı içerik, farklı ad/klasör | `tarama_butunlesme::ayni_icerik_farkli_adlar_kopya_grubuna_girir` |
| Farklı içerik, aynı boyut | `tarama_butunlesme::farkli_icerik_ayni_boyut_kopya_sayilmaz` |
| Boş dosya (0 bayt) | `tarama_butunlesme::bos_dosya_ve_sifir_bayt_ayni_kurala_tabi` |
| Boş dosya kuralı kapalıyken 0 bayt | `tarama_butunlesme::sifir_bayt_dosyalari_bos_dosyalar_kapaliyken_aday_olur` |
| Büyük dosya (8 MiB) + akış kuyruğu sabit belleği | `tarama_butunlesme::buyuk_dosya_akis_kuyrugu_sinirini_asmaz` |
| CRC/önizleme çakışması → 3. aşama ayırır | `motor::tests::motor_crc_cakismasinda_ucuncu_asamaya_gecer_ve_ayirir` |
| Silme yerine taşıma + arşiv doğrulama | `arsiv_geri_alma::arsivleme_kaynaklari_yerinde_birakir` |
| Geri alma ve hedef çakışması | `arsiv_geri_alma::arsiv_geri_alma_var_olan_hedefin_ustune_yazmaz` |
| Bozuk arşiv kopyası geri almayı durdurur | `arsiv_geri_alma::arsiv_bozuk_kopya_geri_almayi_durdurur` |
| Bozuk/yarım manifest | `arsiv_geri_alma::bozuk_manifest_acsiz_kalmaz` |
| Yazma izni (yol) hatası arşivi yarım bırakmaz | `arsiv_geri_alma::yazma_izni_hatasi_arsivi_yarim_birakmaz` |
| Ad çakışması (aynı ad, farklı klasör) | `arsiv_geri_alma::ad_cakismasi_arsivde_kaybolmaz` |
| Türetilmiş dosya adları | `tarama_butunlesme::turetilmis_dosya_adlari_tespit_edilir` |
| Gömülü dosya kuralı açık/kapalı | `tarama_butunlesme::gomulu_dosya_kurali_acik_kapali_denenir` |
| Korunacak uzantı kuralı açık/kapalı | `tarama_butunlesme::korunacak_uzanti_kurali_acik_kapali_denenir` |
| JSON şema doğrulaması (gidiş-dönüş) | `rapor::tests::rapor_json_semasi_geri_okunur` |
| Hatalı yol (yok / dosya) | `tarama_butunlesme::hatali_yol_hatasi_dondurur` |
| Çok derin dizin (40 kademe) | `tarama_butunlesme::cok_derin_dizin_taranir` |
| Windows uzun yol | `tarama_butunlesme::windows_uzun_yolu_islenir_veya_hata_dondurur` |
| Kısmi tarama iptali | `tarama_butunlesme::kismi_tarama_iptali_yarim_sonuc_dondurur` |
| BLAKE3 resmî test vektörleri (0/1024/102400 bayt) | `karma::tests::tam_karma_resmi_blake3_vektoru_*` |
| CRC-32 ISO 3309 kontrol değeri | `karma::tests::crc32_kontrol_degeri_iso3309` |
| Kademeli aşamanın okuma tasarrufu | `tarama_butunlesme::kademeli_aday_uretimi_tam_okumayi_engeller` |

### "Silme yok, taşıma var" nasıl doğrulanıyor?

Üç ayrı katmanda:

1. **Davranışsal:** `arsiv_geri_alma::arsivleme_kaynaklari_yerinde_birakir`
   arşivlemeden sonra üç kaynağın da mevcut ve değişmez olduğunu doğrular;
   `tarama_ve_arsivleme_kaynagi_bozmaz` arşivleme öncesi/sonrası bayt bayt
   karşılaştırır.
2. **Kaynak düzeyinde:** `tests/silme_olmaz_kaniti.rs`, `src/` altındaki tüm
   Rust dosyalarını tarar ve `remove_file`, `remove_dir`, `remove_dir_all`,
   `truncate`, `set_len`, `persist(`, `Command::new` ve `unsafe` kalıplarının
   **bulunmadığını** doğrular (her dosyanın `#[cfg(test)]` modülünden sonrası
   taranır, yani test temizlik kodu kapsam dışıdır). İleride bir `remove_file`
   eklendiği anda bu test kırılır.
3. **Biçim düzeyinde:** `arsiv_kayitlari_her_zaman_kaynak_yolu_tasiyor` testi,
   geri almayı mümkün kılan `kaynak_yol`/`blake3`/`crc32` alanlarının arşiv
   şemasında bulunduğunu doğrular.

Ek olarak arşiv yazımı iki adımlıdır: dosyalar kopyalanır → `manifest.json.partial`
yazılır → arşivdeki her dosya yeniden okunup karması karşılaştırılır → ancak
doğrulama geçerse manifest `manifest.json` adına yeniden adlandırılır.

## Proje Yapısı

```
08-duphunter/
├── Cargo.toml
├── Cargo.lock
├── LICENSE.txt
├── README.md
├── .gitignore
├── src/
│   ├── lib.rs          çekirdek mantık, modül yönlendirmeleri
│   ├── main.rs         CLI kabuğu (clap: scan, hash-report, restore)
│   ├── hata.rs         Hata tipi ve Sonuc takma adı
│   ├── yol.rs          uzun yol, joker desen, ad normalleştirme
│   ├── kurallar.rs     kullanıcı kuralları + JSON kural dosyası
│   ├── gezgin.rs       özyinelemeli dizin gezgini + iptal bayrağı
│   ├── karma.rs        akış kuyruğu, kayan pencere, BLAKE3, CRC32
│   ├── grup.rs         kademeli aday üretimi, gruplar, kararlar
│   ├── motor.rs        üç aşamayı birleştiren motor + okuma sayacı
│   ├── arsiv.rs        kurtarma arşivi, doğrulama, geri alma
│   └── rapor.rs        JSON + Markdown kanıt raporu
└── tests/
    ├── yardimci/mod.rs          geçici dizin yardımcısı (crate bağımlılığı yok)
    ├── tarama_butunlesme.rs     uçtan uca tarama testleri
    ├── arsiv_geri_alma.rs       arşiv ve geri alma testleri
    └── silme_olmaz_kaniti.rs    "silme yok" kuralının kaynak düzeyinde kanıtı
```

## Yapılandırma

### Komut satırı bayrakları (`scan` ve `hash-report`)

| Bayrak | Varsayılan | Etkisi |
|---|---|---|
| `<KOK>` | zorunlu | Taranacak dizin. Dosya verilirse hata verir. |
| `--kurallar <DOSYA>` | yok | JSON kural dosyası. Verilmezse öntanılan kurallar. |
| `--min-boyut <BAYT>` | 1 | Bu boyutun altındaki dosyalar aday olmaz. |
| `--bos-dosyalar` | kapalı | 0 baytlık dosyaları da taramaya al. |
| `--gomulu-atla` | kapalı | Nokta ile başlayan veya gizli nitelikli dosyaları atla. |
| `--turetilmis-atla` | açık | `.tmp`, `~$`, `~`, `.bak` gibi adları atla. |
| `--turetilmis-al` | kapalı | Türetilmiş adlı dosyaları taramaya al (`--turetilmis-atla` ile çelişir). |
| `--koru-uzanti <UZANTI>` | yok | Bu uzantıdaki kopyalar asla arşivlenmez. Birden fazla verilebilir. |
| `--onizleme-bayt <BAYT>` | 65536 | 2. aşamada dosyanın baş ve sonundan okunacak bayt. |
| `--tampon-bayt <BAYT>` | 1048576 | Akış kuyruğunun dilim boyutu. |
| `--rapor-json <DOSYA>` | yok | JSON kanıt raporunun yolu. |
| `--rapor-markdown <DOSYA>` | yok | Markdown kanıt raporunun yolu. |
| `--arsiv <DIZIN>` | yok | Kurtarma arşivi buraya yazılır; verilirse arşivleme yapılır. |
| `--koru <YOL>` | yok | Bu yol grup içinde korunacak kopya olur. Birden fazla verilebilir. |
| `--ozet` | yok | Grup ayrıntısını basmadan yalnızca özeti yazdır. |

### `restore` bayrakları

| Bayrak | Varsayılan | Etkisi |
|---|---|---|
| `<ARSIV>` | zorunlu | `manifest.json` içeren kurtarma arşivi dizini. |
| `--hedef <DIZIN>` | yok | Dosyaları özgün yolları yerine bu dizine geri koy. |

### Kural dosyası şeması

```json
{
  "minimum_boyut": 1,
  "bos_dosyalari_atla": true,
  "gomulu_dosyalari_atla": false,
  "turetilmis_adi_atla": true,
  "korunacak_uzantilar": ["keep", "imza"],
  "haric_yol_desenleri": [
    "*/.git/*",
    "*/node_modules/*",
    "*/$RECYCLE.BIN/*",
    "*/System Volume Information/*"
  ],
  "onizleme_bayt": 65536,
  "tampon_bayt": 1048576
}
```

- Bilinmeyen alan görülürse dosya **reddedilir** (`deny_unknown_fields`);
  yarım okunmuş bir kural dosyasının daha az koruma uygulaması kabul edilmez.
- Eksik alanlar öntanılan değerle doldurulur.
- `onizleme_bayt` ve `tampon_bayt` sıfır olamaz.
- Yol desenlerinde `*` sıfır veya daha çok karakteri, `?` tam bir karakteri
  temsil eder; karşılaştırma büyük/küçük harfe duyarsızdır.

Örnek kural dosyası üretmek için `Kurallar::varsayilan().kaydet(yol)`
kullanılabilir; tüm kurallar JSON olarak okunabilir biçimde yazılır.

## Bilinen Sınırlamalar

**Ertelenen özellikler (MANIFEST.md 08 kartı):**

- Algısal karma (görüntü) — yeniden sıkıştırılmış kopyalar bulunamaz.
- Algısal karma (ses).
- Gerçek silme — araç **yok sayar**; disk alanı kullanıcı tarafından
  serbest bırakılır (raporlanan kazanç bu iş için kanıttır).
- Kanıt raporunun imzalanması.
- JPEG/PNG başlık çözücüsü.

**Ölçülmemiş iddialar:**

- Raporun "≤ 96 MB tepe RSS" bellek bütçesi, 4 GB dosyada RSS artışının
  ≤ 20 MB kalacağı ve 2 GB altı makine kabul testi **ölçülmedi**. Testlerde
  8 MiB bir dosyada `AkisKuyrugu::ayrilan_bellek()` değerinin sabit kaldığı
  doğrulanır; 250 ms aralıkla RSS örneklemesi ve donanım sınıfı ölçümleri
  yapılmamıştır.
- MSRV 1.74 beyan edilmiştir ancak yalnızca rustc 1.98.1 ile derlenmiştir.
- Windows uzun yol testi, yolun açılıp açılmadığına göre iki olası sonucu
  kabul eder (açılıyorsa karma doğrulanır, açılmıyorsa panik değil düzgün bir
  `Hata::Io` döndüğü doğrulanır). Bu ortamda uzun yol açıldı.

**Diğer sınırlar:**

- Sembolik bağlar ve sabit disk bağları **izlenmez**; sayımları bozmasın diye
  tarama dışında bırakılır ve `Tarama::baglantilar` listesinde raporlanır.
- Azami dizin derinliği 64'tür; aşan dizinler açılmaz ve hata listesine yazılır.
- `hash-report` komutunun JSON çıktısı `scan --rapor-json` ile aynı şemadadır.
- **Tek `#[allow]` kullanımı:** `src/` içindeki her `#[cfg(test)] mod tests`
  üzerinde `#[allow(clippy::unwrap_used, clippy::expect_used)]` vardır.
  Gerekçe: crate seviyesinde bu lintler açıktır ve üretim kodunda `unwrap`/
  `expect` kullanılmaz; `expect` yalnızca testin başarısızlık mesajıdır.
  `cargo clippy -- -D warnings` (hedefsiz) ve `cargo clippy --all-targets
  -- -D warnings` (hedefli) ikisi de temizdir. Başka bir `#[allow]` yoktur.
- Geçici test dizinleri `Drop` ile temizlenir; temizlik hatası `Drop`
  içinden döndürülemediği için `let _ =` ile bilinçli olarak yutulur.
- Paralel tarama (iş parçacığı) uygulanmadı; tarama tek iş parçacıklıdır.
- Arşiv sıkıştırılmaz: dosyalar olduğu gibi kopyalanır. Bu, disk alanını
  artırır ama bozulma riskini ve geri yükleme süresini düşürür.
- Zaman damgaları Unix saniyesi çözünürlüğündedir; `chrono`/`time` kullanılmaz.

## Gelecek Geliştirmeler

1. Algısal karma (görüntü) — küçültülmüş görüntü üzerinden DCT; birebir
   kopya raporundan **ayrı** bir başlık altında, "kopya" değil "benzer"
   diliyle sunulmalı.
2. Tarama önbelleği: boyut + mtime + boyut/karma eşlemesiyle tekrar taramada
   yalnızca değişen dosyaların yeniden karmalanması.
3. Çok iş parçacıklı tarama (raporun "çekirdek sayısı, tavan 8" kararı) ve
   `drop`-lı arşiv yazımı.
4. `--koru` yerine etkileşimli grup ekranı (terminal seçim listesi).
5. Boş alan öncesi denetimi: arşiv için gereken alanın 1,2 katı yoksa işlem
   başlamasın.
6. Algısal karma (ses), raporun açık sorularındaki eşik seçimi çalışmasıyla
   birlikte.
7. Bağımsız geri yükleme aracı: arşiv biçiminin uzun ömürlü olması için.

## Troubleshooting

**1. Belirti:** `hata: tarama kökü okuma başarısız (C:\yok-boyle-bir-dizin):
Sistem belirtilen dosyayı bulamıyor. (os error 2)`
**Neden:** Verilen dizin yok veya erişilemiyor.
**Çözüm:** Yolu kontrol edin. Tırnak içinde verin (`"C:\Program Files\..."`).
Ağ sürücüsüyse bağlantının açık olduğundan emin olun. Çıkış kodu `1`'dir.

**2. Belirti:** `hata: tarama yolu geçersiz (...tutanak.txt): verilen yol bir
dizin değil`
**Neden:** `<KOK>` olarak dosya verilmiş. Araç yalnızca dizin tarar.
**Çözüm:** Dosyanın bulunduğu dizini verin.

**3. Belirti:** `hata: kurtarma arşivi bozuk (...manifest.json): key must be a
string at line 1 column 3`
**Neden:** Manifest bozuk, kesilmiş ya da farklı bir sürümle yazılmış.
**Çözüm:** Arşiv doğrulanmadan kullanılamaz. Kaynak dosyalar hâlâ yerindedir
(araç hiçbir şeyi silmez), bu yüzden **önce arşiv klasörünü yedekleyin**,
sonra `duphunter restore` ile doğrulayın. Arşiv açılamıyorsa
`manifest.json.partial` dosyası olup olmadığını kontrol edin: varsa yazma
yarım kalmıştır ve arşiv geçerli sayılmaz.

**4. Belirti:** `hata: geri alma hedefi zaten var, üzerine yazılmadı: ...`
**Neden:** Geri alınacak konumda dosya var. Araç üzerine yazmayı reddeder.
**Çözüm:** `--hedef` ile boş bir dizin verin, ya da hedefteki dosyayı elle
taşıyın ve komutu yeniden çalıştırın.

**5. Belirti:** Tarama çok yavaş ve `Okunan bayt` taranan toplama yakın.
**Neden:** Dosyalar çoğunlukla tekil boyutta değil, 2. aşamaya giriyor ve
2. aşama da dosyanın tamamını okuyor (`2 × onizleme_bayt`'ten küçük
dosyalarda bu böyledir).
**Çözüm:** `--onizleme-bayt` değerini düşürün (ör. 4096) veya `--min-boyut`
ile küçük dosyaları eleyin.

**6. Belirti:** Gerçek kopyalar bulunmuyor / çok fazla `dışlanan` var.
**Neden:** Türetilmiş ad kuralı, `.git` desenleri veya minimum boyut devrede.
**Çözüm:** `--turetilmis-al` ile geçici adları tarama alın, `--kurallar` ile
`haric_yol_desenleri` listesini daraltın, çıktının altındaki "Dışlanan
dosyalar" tablosunu Markdown raporda inceleyin.

## Atıflar

- **BLAKE3** — Jack O'Connor ve BLAKE3 ekibi, <https://github.com/BLAKE3-team/BLAKE3>.
  Kullanılan Rust crate'i: <https://docs.rs/blake3/>. BLAKE3, kamu malı
  (CC0) **ve** Apache-2.0 olarak çift lisanslıdır; MIT dağıtımımla çelişmez.
- **BLAKE3 test vektörleri** — <https://github.com/BLAKE3-team/BLAKE3/blob/master/test_vectors/test_vectors.json>.
  Testler bu dosyadaki 0, 1024 ve 102400 bayt girdilerinin resmî çıktılarını
  doğrudan karşılaştırır.
- **BLAKE3 makalesi** — Jack O'Connor, Jean-Philippe Aumasson, Samuel Neves,
  Zooko Wilcox, "BLAKE3: One Function, Fast Everywhere",
  <https://github.com/BLAKE3-team/BLAKE3/blob/master/paper/BLAKE3.pdf>.
- **CRC-32 (ISO 3309)** — kontrol değeri `CRC("123456789") = 0xCBF43926`
  standardın tanımladığı "check" değeridir; `crc32fast` crate'i:
  <https://docs.rs/crc32fast/>.
- **Karmaların neden kriptografik olması gerektiği** — bu projede
  "birebir kopya" iddiasının kanıtı BLAKE3'ten gelir; karar D-008.
- **Rust standart kütüphanesi** — <https://doc.rust-lang.org/std/>
  (`std::io::Read`, `SeekFrom`, `std::fs`, `std::os::windows::fs::MetadataExt`).
- **serde / serde_json** — <https://serde.rs/> · <https://github.com/serde-rs/json>
- **clap** — <https://docs.rs/clap/>
- **Referans araçlar** (kavramsal karşılaştırma için, kod kopyalanmadı):
  fdupes <https://github.com/ilanschnell/fdupes>,
  rmlint <https://github.com/sahib/rmlint>,
  dupeGuru <https://dupeguru.voltaicideas.net/>
- **Fikir raporu** (iç tasarımın kaynağı, URL değil yerel yol):
  `%USERPROFILE%\Desktop\Fikirler\08-kopya-avcisi.html`

## Lisans

MIT — tam metin `LICENSE.txt` dosyasındadır. Telif satırı:
`Copyright (c) 2026 DupHunter contributors`.
