//! Helpers puros para as métricas do protótipo do frame.
//! Nenhuma dependência de Wayland: testável isoladamente.

/// p95 (ou outro percentil) de uma amostra. `p` em [0,1].
/// Ordena a própria fatia; a amostra é descartável.
pub fn percentile(values: &mut [f64], p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let rank = (p * (values.len() - 1) as f64).round() as usize;
    values[rank]
}

/// Descompasso: conteúdo e decoração com tamanhos diferentes.
/// No protótipo, o tamanho do frame é o configurado; o do conteúdo é o buffer
/// commitado. Se divergem, o usuário vê conteúdo esticado ou borda desalinhada.
pub fn is_descompasso(content: (u32, u32), frame: (u32, u32)) -> bool {
    content != frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p95_de_cinco_valores() {
        let mut v = vec![1.0, 2.0, 3.0, 4.0, 100.0];
        assert_eq!(percentile(&mut v, 0.95), 100.0);
        assert_eq!(percentile(&mut v, 0.5), 3.0);
    }

    #[test]
    fn descompasso_detecta_divergencia() {
        assert!(!is_descompasso((320, 200), (320, 200)));
        assert!(is_descompasso((300, 180), (320, 200)));
    }
}
