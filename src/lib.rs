//! DupHunter — akış tabanlı karmalarla birebir kopya avcısı.
//!
//! Araç üç aşamalı aday üretimi kullanır: (1) boyuta göre gruplama (hiç okuma
//! yapılmaz), (2) dosyanın baş ve son penceresinden hızlı ön eleme (CRC32 ve
//! BLAKE3 önizleme özeti), (3) yalnızca eşleşenler için tam akış BLAKE3.
//! Her dosya sabit boyutlu bir tampon üzerinden okunur; hiçbir dosya belleğe
//! tamamen alınmaz.
//!
//! **Güvenlik modeli:** DupHunter hiçbir koşulda dosya silmez. Karar verilen
//! kopyalar kurtarma arşivine kopyalanır, kaynak yerinde bırakılır ve
//! `arsiv` modülündeki geri alma ile özgün yoluna geri alınabilir.
//!
//! Modüller:
//!
//! - [`hata`]: hata tipi ve `Sonuc` takma adı.
//! - [`yol`]: yol normalleştirme, joker desen ve uzun yol yardımcıları.
//! - [`kurallar`]: kullanıcı kuralları ve JSON kural dosyası.
//! - [`gezgin`]: özyinelemeli dizin gezgini ve iptal bayrağı.
//! - [`karma`]: sabit bellekli akış kuyruğu, kayan pencere, BLAKE3 ve CRC32.
//! - [`grup`]: kademeli aday üretimi, kopya grupları ve kararlar.
//! - [`motor`]: üç aşamayı birleştiren tarama motoru ve okuma sayacı.
//! - [`arsiv`]: kurtarma arşivi, doğrulama ve geri alma.
//! - [`rapor`]: JSON ve Markdown kanıt raporu.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod arsiv;
pub mod gezgin;
pub mod grup;
pub mod hata;
pub mod karma;
pub mod kurallar;
pub mod motor;
pub mod rapor;
pub mod yol;

pub use arsiv::Arsiv;
pub use gezgin::{IptalBayragi, Tarama};
pub use grup::{Grup, Karar, KararTuru};
pub use hata::{Hata, Sonuc};
pub use karma::AkisKuyrugu;
pub use kurallar::Kurallar;
pub use motor::MotorSonucu;
pub use rapor::TaramaRaporu;

/// Aracın sürüm dizesi.
pub const SURUM: &str = env!("CARGO_PKG_VERSION");
