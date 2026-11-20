//! Diagnósticos nomeados do resize do frame (Tarefa 02j).
//!
//! Lógica pura, sem Wayland: testável isoladamente. As linhas de log só são
//! formatadas sob `cfg(feature = "bench")` para não existirem no binário de
//! produção (critério 6 do cartão). Nenhuma política de resize/move muda aqui.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// Instante de parede em nanossegundos desde o epoch, para o `t=` dos logs.
#[cfg(feature = "bench")]
pub fn now_ns() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Configure sem `ack_configure` neste prazo.
pub const ACK_TIMEOUT: Duration = Duration::from_millis(100);
/// Configure sem buffer no tamanho alvo neste prazo.
pub const DRAW_TIMEOUT: Duration = Duration::from_millis(300);
/// Episódio de escala sem nova atividade é fechado neste prazo.
pub const STRETCH_IDLE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    NeverAcked,
    NeverDrewAtSize,
    StretchActive,
}

/// Um evento nomeado. Todos os campos pedidos pelo cartão estão presentes;
/// campos sem significado para um dado modo ficam com o valor neutro indicado.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub kind: Kind,
    pub window_id: u32,
    pub serial: u32,
    /// Tamanho de CONTEÚDO do configure.
    pub w: u32,
    pub h: u32,
    /// Tamanho de BUFFER esperado (conteúdo + insets/shadow).
    pub target: (u32, u32),
    /// Último buffer conhecido para esta janela.
    pub buffer: (u32, u32),
    pub waited_ms: u128,
    pub start_ns: u128,
    pub end_ns: u128,
    pub duration_ms: u128,
    pub max_scale: f64,
}

#[derive(Debug, Clone)]
struct Pending {
    window_id: u32,
    serial: u32,
    content: (u32, u32),
    target: (u32, u32),
    configured_at: Instant,
    configured_at_ns: u128,
    acked: bool,
    drew: bool,
    ack_logged: bool,
    draw_logged: bool,
}

#[derive(Debug, Clone)]
struct Stretch {
    start: Instant,
    start_ns: u128,
    last: Instant,
    last_ns: u128,
    max_scale: f64,
    serial: u32,
    content: (u32, u32),
    target: (u32, u32),
}

#[derive(Default)]
pub struct FrameDiagnostics {
    pending: Vec<Pending>,
    last_serial: HashMap<u32, u32>,
    last_content: HashMap<u32, (u32, u32)>,
    last_target: HashMap<u32, (u32, u32)>,
    last_buffer: HashMap<u32, (u32, u32)>,
    stretch: HashMap<u32, Stretch>,
    events: Vec<Diagnostic>,
    suppress_ack_once: bool,
    /// Serial cujo ack deve ser ignorado na fase `faults` (todos os acks dele).
    suppress_ack_serial: Option<u32>,
    suppress_draw_once: bool,
}

impl FrameDiagnostics {
    /// Injeção de falha SOMENTE sob `bench`: força um configure sem ack e um
    /// configure com alvo impossível, para os dois primeiros diagnósticos
    /// deixarem de depender de corrida na VM.
    #[cfg(feature = "bench")]
    pub fn with_fault_injection(suppress_ack: bool, suppress_draw: bool) -> Self {
        Self {
            suppress_ack_once: suppress_ack,
            suppress_draw_once: suppress_draw,
            ..Self::default()
        }
    }

    pub fn note_configure(
        &mut self,
        window_id: u32,
        serial: u32,
        content: (u32, u32),
        target: (u32, u32),
        now: Instant,
        now_ns: u128,
    ) {
        self.last_serial.insert(window_id, serial);
        self.last_content.insert(window_id, content);
        // Injeção de falha bench: nenhum buffer do WebKit pode alcançar este alvo.
        let target = if self.suppress_draw_once {
            self.suppress_draw_once = false;
            (
                target.0.saturating_add(100_000),
                target.1.saturating_add(100_000),
            )
        } else {
            target
        };
        self.last_target.insert(window_id, target);
        self.pending.push(Pending {
            window_id,
            serial,
            content,
            target,
            configured_at: now,
            configured_at_ns: now_ns,
            acked: false,
            drew: false,
            ack_logged: false,
            draw_logged: false,
        });
    }

    pub fn note_ack(&mut self, serial: u32) {
        if self.suppress_ack_once && self.pending.iter().any(|p| p.serial == serial) {
            self.suppress_ack_serial = Some(serial);
            self.suppress_ack_once = false;
        }
        if self.suppress_ack_serial == Some(serial) {
            return;
        }
        if let Some(p) = self
            .pending
            .iter_mut()
            .find(|p| p.serial == serial && !p.acked)
        {
            p.acked = true;
        }
    }

    pub fn note_buffer(&mut self, window_id: u32, buffer: (u32, u32)) {
        self.last_buffer.insert(window_id, buffer);
        for p in self
            .pending
            .iter_mut()
            .filter(|p| p.window_id == window_id && p.target == buffer)
        {
            p.drew = true;
        }
    }

    pub fn note_stretch(
        &mut self,
        window_id: u32,
        active: bool,
        scale: f64,
        now: Instant,
        now_ns: u128,
    ) {
        if active {
            let serial = self.last_serial.get(&window_id).copied().unwrap_or(0);
            let content = self.last_content.get(&window_id).copied().unwrap_or((0, 0));
            let target = self.last_target.get(&window_id).copied().unwrap_or((0, 0));
            let entry = self.stretch.entry(window_id).or_insert_with(|| Stretch {
                start: now,
                start_ns: now_ns,
                last: now,
                last_ns: now_ns,
                max_scale: scale,
                serial,
                content,
                target,
            });
            entry.last = now;
            entry.last_ns = now_ns;
            entry.max_scale = entry.max_scale.max(scale);
        } else if self.stretch.contains_key(&window_id) {
            self.close_stretch(window_id, now, now_ns);
        }
    }

    fn close_stretch(&mut self, window_id: u32, end: Instant, end_ns: u128) {
        if let Some(s) = self.stretch.remove(&window_id) {
            let duration_ms = end.saturating_duration_since(s.start).as_millis();
            self.events.push(Diagnostic {
                kind: Kind::StretchActive,
                window_id,
                serial: s.serial,
                w: s.content.0,
                h: s.content.1,
                target: s.target,
                buffer: (0, 0),
                waited_ms: 0,
                start_ns: s.start_ns,
                end_ns,
                duration_ms,
                max_scale: s.max_scale,
            });
        }
    }

    /// Chamado no idle: emite os timeouts vencidos, no máximo uma vez por serial.
    pub fn tick(&mut self, now: Instant, now_ns: u128) {
        for p in self.pending.iter_mut() {
            let waited = now.saturating_duration_since(p.configured_at);
            if !p.acked && !p.ack_logged && waited >= ACK_TIMEOUT {
                p.ack_logged = true;
                let (window_id, serial, content, target, start_ns) = (
                    p.window_id,
                    p.serial,
                    p.content,
                    p.target,
                    p.configured_at_ns,
                );
                let buffer = self.last_buffer.get(&window_id).copied().unwrap_or((0, 0));
                self.events.push(Diagnostic {
                    kind: Kind::NeverAcked,
                    window_id,
                    serial,
                    w: content.0,
                    h: content.1,
                    target,
                    buffer,
                    waited_ms: waited.as_millis(),
                    start_ns,
                    end_ns: now_ns,
                    duration_ms: 0,
                    max_scale: 0.0,
                });
            }
            if !p.drew && !p.draw_logged && waited >= DRAW_TIMEOUT {
                p.draw_logged = true;
                let (window_id, serial, content, target, start_ns) = (
                    p.window_id,
                    p.serial,
                    p.content,
                    p.target,
                    p.configured_at_ns,
                );
                let buffer = self.last_buffer.get(&window_id).copied().unwrap_or((0, 0));
                self.events.push(Diagnostic {
                    kind: Kind::NeverDrewAtSize,
                    window_id,
                    serial,
                    w: content.0,
                    h: content.1,
                    target,
                    buffer,
                    waited_ms: waited.as_millis(),
                    start_ns,
                    end_ns: now_ns,
                    duration_ms: 0,
                    max_scale: 0.0,
                });
            }
        }
        self.pending
            .retain(|p| !((p.acked || p.ack_logged) && (p.drew || p.draw_logged)));

        let idle: Vec<u32> = self
            .stretch
            .iter()
            .filter(|(_, s)| now.saturating_duration_since(s.last) >= STRETCH_IDLE)
            .map(|(id, _)| *id)
            .collect();
        for id in idle {
            if let Some(s) = self.stretch.get(&id) {
                let (end, end_ns) = (s.last, s.last_ns);
                self.close_stretch(id, end, end_ns);
            }
        }
    }

    /// Chamado no shutdown: fecha episódios de escala ainda abertos no último
    /// frame observado (não infla a duração com o tempo de encerramento).
    #[cfg(feature = "bench")]
    pub fn flush(&mut self, _now: Instant, _now_ns: u128) {
        let ids: Vec<u32> = self.stretch.keys().copied().collect();
        for id in ids {
            if let Some(s) = self.stretch.get(&id) {
                let (end, end_ns) = (s.last, s.last_ns);
                self.close_stretch(id, end, end_ns);
            }
        }
    }

    pub fn drain_events(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.events)
    }

    /// Número de configures ainda sob observação (usado pelo teste de ack).
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(feature = "bench")]
impl Diagnostic {
    /// Formata a linha nomeada. Só existe com `bench`.
    pub fn render(&self) -> String {
        let name = match self.kind {
            Kind::NeverAcked => "FRAME_NEVER_ACKED",
            Kind::NeverDrewAtSize => "FRAME_NEVER_DREW_AT_SIZE",
            Kind::StretchActive => "FRAME_STRETCH_ACTIVE",
        };
        format!(
            "{name} t={} serial={} window_id={} w={} h={} target_w={} target_h={} \
             buffer_w={} buffer_h={} waited_ms={} start_t={} end_t={} duration_ms={} \
             max_scale={:.3}",
            self.end_ns,
            self.serial,
            self.window_id,
            self.w,
            self.h,
            self.target.0,
            self.target.1,
            self.buffer.0,
            self.buffer.1,
            self.waited_ms,
            self.start_ns,
            self.end_ns,
            self.duration_ms,
            self.max_scale,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    fn configure(d: &mut FrameDiagnostics, t: Instant) {
        d.note_configure(7, 42, (300, 200), (324, 236), t, 1_000);
    }

    #[test]
    fn never_acked_uma_vez_apos_100ms() {
        let start = t0();
        let mut d = FrameDiagnostics::default();
        configure(&mut d, start);
        d.tick(start + Duration::from_millis(99), 1_099);
        assert!(d.drain_events().is_empty());
        d.tick(start + Duration::from_millis(100), 1_100);
        let evs = d.drain_events();
        assert_eq!(evs.len(), 1, "esperado 1 FRAME_NEVER_ACKED: {evs:?}");
        assert_eq!(evs[0].kind, Kind::NeverAcked);
        assert_eq!(evs[0].window_id, 7);
        assert_eq!(evs[0].serial, 42);
        assert_eq!((evs[0].w, evs[0].h), (300, 200));
        assert_eq!(evs[0].target, (324, 236));
        assert!(evs[0].waited_ms >= 100);
        d.tick(start + Duration::from_millis(250), 1_250);
        assert!(d.drain_events().is_empty(), "diagnóstico duplicado");
    }

    #[test]
    fn ack_antes_do_timeout_nao_emite() {
        let start = t0();
        let mut d = FrameDiagnostics::default();
        configure(&mut d, start);
        assert_eq!(d.pending_count(), 1, "configure não foi registrado");
        d.note_ack(42);
        d.note_buffer(7, (324, 236));
        d.tick(start + Duration::from_millis(150), 1_150);
        assert!(d.drain_events().is_empty());
        assert_eq!(
            d.pending_count(),
            0,
            "configure ackado e desenhado não foi limpo"
        );
    }

    #[test]
    fn never_drew_at_size_apos_300ms() {
        let start = t0();
        let mut d = FrameDiagnostics::default();
        configure(&mut d, start);
        d.note_ack(42);
        d.note_buffer(7, (300, 200));
        d.tick(start + Duration::from_millis(299), 1_299);
        assert!(d.drain_events().is_empty());
        d.tick(start + Duration::from_millis(300), 1_300);
        let evs = d.drain_events();
        assert_eq!(evs.len(), 1, "esperado 1 FRAME_NEVER_DREW_AT_SIZE: {evs:?}");
        assert_eq!(evs[0].kind, Kind::NeverDrewAtSize);
        assert_eq!(evs[0].serial, 42);
        assert_eq!(evs[0].target, (324, 236));
        assert_eq!(evs[0].buffer, (300, 200));
        assert!(evs[0].waited_ms >= 300);
        d.tick(start + Duration::from_millis(500), 1_500);
        assert!(d.drain_events().is_empty(), "diagnóstico duplicado");
    }

    #[test]
    fn stretch_um_evento_por_episodio() {
        let start = t0();
        let mut d = FrameDiagnostics::default();
        configure(&mut d, start);
        d.note_stretch(7, true, 1.4, start, 1_000);
        d.note_stretch(7, true, 1.8, start + Duration::from_millis(50), 1_050);
        d.note_stretch(7, false, 1.0, start + Duration::from_millis(80), 1_080);
        let evs = d.drain_events();
        assert_eq!(evs.len(), 1, "esperado 1 FRAME_STRETCH_ACTIVE: {evs:?}");
        assert_eq!(evs[0].kind, Kind::StretchActive);
        assert_eq!(evs[0].window_id, 7);
        assert_eq!(evs[0].serial, 42);
        assert_eq!(evs[0].duration_ms, 80);
        assert!((evs[0].max_scale - 1.8).abs() < 1e-9);
        // Segundo episódio, separado, gera outra linha.
        d.note_stretch(7, true, 1.2, start + Duration::from_millis(200), 1_200);
        d.note_stretch(7, false, 1.0, start + Duration::from_millis(230), 1_230);
        assert_eq!(d.drain_events().len(), 1);
    }
}
