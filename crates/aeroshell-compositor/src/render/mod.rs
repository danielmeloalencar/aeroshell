//! Composição de elementos de render compartilhada entre backends.
//!
//! Esta árvore não conhece winit, udev nem headless: só Smithay e o estado do
//! domínio. Criada na F1a para que o backend udev reutilize exatamente o mesmo
//! caminho de 9-slice/placeholder do nested.

pub mod elements;
