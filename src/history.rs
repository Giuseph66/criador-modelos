//! Histórico para desfazer (Ctrl+Z) e refazer (Ctrl+Y).
//!
//! Guarda versões anteriores como `Arc<T>`. Com `Arc::make_mut`, a versão
//! atual só é copiada quando alguém vai alterá-la e o histórico ainda
//! aponta para ela ("copy-on-write"): versões iguais não gastam memória.

use std::sync::Arc;

pub struct History<T> {
    undo: Vec<Arc<T>>,
    redo: Vec<Arc<T>>,
    limit: usize,
}

impl<T> History<T> {
    /// `limit`: quantos passos de desfazer guardar.
    pub fn new(limit: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), limit: limit.max(1) }
    }

    /// Registra o estado *antes* de uma alteração. Limpa o "refazer".
    pub fn push(&mut self, before: Arc<T>) {
        self.undo.push(before);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Volta um passo: recebe o estado atual e devolve o anterior.
    pub fn undo(&mut self, current: Arc<T>) -> Option<Arc<T>> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    /// Avança um passo: recebe o estado atual e devolve o seguinte.
    pub fn redo(&mut self, current: Arc<T>) -> Option<Arc<T>> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desfaz_e_refaz() {
        let mut h = History::new(10);
        let mut state = Arc::new(1);

        h.push(state.clone());
        state = Arc::new(2);
        h.push(state.clone());
        state = Arc::new(3);

        state = h.undo(state).unwrap();
        assert_eq!(*state, 2);
        state = h.undo(state).unwrap();
        assert_eq!(*state, 1);
        assert!(h.undo(state.clone()).is_none());

        state = h.redo(state).unwrap();
        assert_eq!(*state, 2);
        state = h.redo(state).unwrap();
        assert_eq!(*state, 3);
        assert!(!h.can_redo());
    }

    #[test]
    fn nova_edicao_limpa_refazer() {
        let mut h = History::new(10);
        h.push(Arc::new(1));
        let state = h.undo(Arc::new(2)).unwrap();
        assert!(h.can_redo());
        h.push(state);
        assert!(!h.can_redo());
    }

    #[test]
    fn respeita_limite() {
        let mut h = History::new(2);
        for i in 0..5 {
            h.push(Arc::new(i));
        }
        let a = h.undo(Arc::new(99)).unwrap();
        let b = h.undo(a).unwrap();
        assert_eq!(*b, 3);
        assert!(!h.can_undo());
    }
}
