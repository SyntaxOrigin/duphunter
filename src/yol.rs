//! Yol yardımcıları: uzun yol güvenli erişim, joker desen eşleme, ad normalleştirme.
//!
//! Bu modül dosya sistemi üzerinde hiçbir işlem yapmaz; yalnızca `Path` ile
//! `str` arasındaki dönüşüm kurallarını toplar ve saf fonksiyonlar sunar.

use std::path::{Path, PathBuf};

/// Windows'un 260 karakterlik klasik yol sınırını aşan bir yolun sabit
/// uzunlukta olup olmadığını dönen eşik değerdir.
pub const UZUN_YOL_ESIGI: usize = 240;

/// `\\?\` önekini taşıyan, "harfi harfine" (verbatim) yol öneki.
#[cfg(windows)]
const VERBATIM_ONEK: &str = r"\\?\";

/// Yolu işletim sisteminin uzun yol kuralına uyacak biçime getirir.
///
/// Windows'ta mutlak ve çok uzun yollar `\\?\` öneki olmadan açılamayabilir;
/// bu fonksiyon yalnızca eşiği aşan mutlak yollara önek ekler ve göreli
/// yollara dokunmaz. Diğer işletim sistemlerinde yol olduğu gibi döner.
pub fn uzun_yol(yol: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let metin = yol.as_os_str().to_string_lossy().to_string();
        let mut mutlak = yol.is_absolute();
        if !mutlak && metin.len() > 1 {
            let baytlar = metin.as_bytes();
            if baytlar[1] == b':' {
                mutlak = true;
            }
        }
        if mutlak && metin.len() > UZUN_YOL_ESIGI && !metin.starts_with(VERBATIM_ONEK) {
            if let Some(kalan) = metin.strip_prefix("\\\\") {
                // UNC yol: \\sunuc\paylasim -> \\?\UNC\sunuc\paylasim
                return PathBuf::from(format!("{}{}\\{}", VERBATIM_ONEK, "UNC", kalan));
            }
            return PathBuf::from(format!("{}{}", VERBATIM_ONEK, metin));
        }
    }
    yol.to_path_buf()
}

/// Joker desen ile metni karşılaştırır.
///
/// `*` sıfır veya daha çok karakteri, `?` tam bir karakteri temsil eder.
/// Karşılaştırma büyük/küçük harfe duyarsızdır ve geriye dönmez; desen
/// metnin ortasında başlıyorsa eşleşme yine de bulunur.
pub fn desen_eslestir(desen: &str, metin: &str) -> bool {
    let d: Vec<char> = desen.to_lowercase().chars().collect();
    let m: Vec<char> = metin.to_lowercase().chars().collect();
    let mut i = 0usize;
    let mut j = 0usize;
    let mut yildiz: Option<usize> = None;
    let mut eslesme = 0usize;

    while j < m.len() {
        if i < d.len() && (d[i] == m[j] || d[i] == '?') {
            i += 1;
            j += 1;
        } else if i < d.len() && d[i] == '*' {
            yildiz = Some(i);
            i += 1;
            eslesme = j;
        } else if let Some(s) = yildiz {
            i = s + 1;
            eslesme += 1;
            j = eslesme;
        } else {
            return false;
        }
    }
    while i < d.len() && d[i] == '*' {
        i += 1;
    }
    i == d.len()
}

/// Yolu karşılaştırma ve desenleme için tek biçime indirger: ters eğik çizgi
/// kullanılır, küçük harfe çevrilir ve sondaki ayraçlar atılır.
pub fn normalleştir(yol: &Path) -> String {
    let metin = yol.to_string_lossy().replace('\\', "/");
    let küçük = metin.to_lowercase();
    let son = küçük.trim_end_matches('/');
    if son.is_empty() {
        "/".to_string()
    } else {
        son.to_string()
    }
}

/// Bir dosya adının uzantısını nokta olmadan, küçük harfe çevrilmiş olarak döndürür.
pub fn uzanti(ad: &str) -> Option<String> {
    let son = ad.rsplit_once('.')?;
    if son.0.is_empty() || son.1.is_empty() {
        return None;
    }
    Some(son.1.to_lowercase())
}

#[cfg(test)]
// Gerekçe: expect/unwrap yalnızca test içinde kullanılır ve testin
// başarısızlık mesajıdır. Üretim kodunda bu lintler açıktır
// (crate seviyesinde clippy::unwrap_used/clippy::expect_used).
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn uzanti_noktasiz_kucuk_harfe_duser() {
        let b = uzanti("RAPOR.PDF");
        match b {
            Some(v) => assert_eq!(v, "pdf"),
            None => panic!("uzanti bulunamadi"),
        }
    }

    #[test]
    fn noktasiz_ad_uzantisizdir() {
        assert!(uzanti("README").is_none());
    }

    #[test]
    fn on_planda_ki_nokta_uzanti_uretmez() {
        assert!(uzanti(".gitignore").is_none());
    }

    #[test]
    fn joker_yildiz_bos_da_eslesir() {
        assert!(desen_eslestir("*.tmp", "notlar.tmp"));
        assert!(desen_eslestir("*.tmp", "notlar.TMP"));
    }

    #[test]
    fn joker_yildiz_bos_da_eslesmez_ve_ortada_calisir() {
        assert!(!desen_eslestir("*.tmp", "notlar.txt"));
        assert!(desen_eslestir("a*c", "abbbc"));
        assert!(!desen_eslestir("a*c", "abbbd"));
    }

    #[test]
    fn joker_soru_isareti_bir_karakter_tutarken_yildiz_geri_gider() {
        assert!(desen_eslestir("a?c", "abc"));
        assert!(!desen_eslestir("a?c", "ac"));
        assert!(desen_eslestir("*.doc?", "rapor.docx"));
    }

    #[test]
    fn normalleştir_egri_duzleştirir_ve_kucultur() {
        let yol = Path::new(r"C:\Kullanicilar\Masaustu\");
        assert_eq!(normalleştir(yol), "c:/kullanicilar/masaustu");
    }

    #[test]
    fn uzun_yol_kisa_yolu_degistirmez() {
        let kisa = PathBuf::from("veriler/a.txt");
        assert_eq!(uzun_yol(&kisa), kisa);
    }
}
