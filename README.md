# Criador de Modelos

App desktop (Rust) que transforma uma imagem em **SVG preto e branco pronto para o slicer** — para fazer chaveiros, placas, carimbos etc.

Substitui o fluxo manual:

```
antes:  imagem → ChatGPT (preto e branco) → site de vetorização → slicer
agora:  imagem → Criador de Modelos → slicer
```

## Como rodar

```bash
cargo run --release                     # abre a janela
cargo run --release -- foto.png         # abre já com uma imagem
cargo run --release --example converter -- foto.png saida.svg 50   # sem janela, 50 mm de largura
cargo test                              # testes
```

O executável fica em `target/release/criador_modelos`.

## Como usar

Fluxo: **abrir imagem → detectar cores → editar → exportar SVG ou STL**.

### 1. Detectar
- **🎨 Cores** (padrão): detecta as N cores principais; cada cor vira uma camada.
- **◐ Preto e branco**: separa claro/escuro (Automático/Otsu, Manual ou Adaptativo para fotos).
- Mudar a detecção depois de editar substitui as edições — `Ctrl+Z` desfaz.

### 2. Editar (aba ✏ Editar)

| Ferramenta | Tecla | O que faz |
|---|---|---|
| ✋ Mover | `H` | Arrasta a imagem. Também: **espaço + arrastar** ou botão do meio/direito |
| 👆 Selecionar | `V` | Clica numa parte da imagem e pega a cor dela |
| 🖊 Pincel | `B` | Pinta com a cor ativa. `[` / `]` mudam o tamanho |
| ⬚ Borracha | `E` | Apaga (vira fundo, fica quadriculado = não entra no modelo) |
| 💧 Balde | `G` | Pinta só a área contínua clicada (ex.: uma letra, uma bolinha) |

- **Zoom**: roda do mouse (mira no cursor), `+` / `-`, `0` = ajustar à tela.
- **Ver original**: diminua o slider para enxergar a imagem original por baixo e pintar por cima dela.
- **Desfazer / Refazer**: `Ctrl+Z` / `Ctrl+Y` (ou `Ctrl+Shift+Z`), até 60 passos.

**Paleta** (painel da esquerda): clique numa cor para torná-la a cor ativa.
- **➕** adiciona uma cor nova (já fica ativa no pincel).
- Seletor de cor troca a cor; **↺ original** volta.
- **Fundo**: a cor não entra no SVG/STL (riscada em vermelho).
- **Juntar com**: os pixels passam para outra cor. **🗑 Apagar cor**: viram fundo.

### 3. Contorno
Suavizar bordas, remover manchas (também fecha furinhos), curvas/retas/pixel.
**Empilhar camadas**: cada camada cobre também a área das de cima → base sólida.

### 4. Exportar
- **💾 Salvar SVG** (`Ctrl+S`): largura em mm = largura do desenho (margens cortadas).
- **📦 Exportar STL** (`Ctrl+E`): extruda as camadas.
  - **base**: espessura da camada de baixo; **relevo**: quanto cada camada de cima sobe.
  - **STL separado por cor**: um arquivo por camada — importe todos juntos no slicer e escolha um filamento para cada.
- **Mais ▾**: um SVG por cor; PNG da imagem editada.

Dica de impressão: traços com menos de ~1 mm (≈ 2 larguras de bico) costumam sumir no fatiamento.

## Como funciona (pipeline)

| Etapa | Arquivo | O que faz |
|---|---|---|
| 1. Preparar | `src/processing.rs` → `prepare_gray` | Redimensiona (reduz grandes, amplia pequenas até 4×), transparência vira branco, converte para cinza |
| 2a. Cores | `src/colors.rs` → `quantize` | k-means agrupa os pixels em N cores |
| 2b. Preto e branco | `src/processing.rs` → `to_black_and_white` | Desfoque gaussiano + limiar (fixo, Otsu ou adaptativo Bradley-Roth) |
| 3. Editar | `src/colors.rs` → `Document` | Cada pixel aponta para uma cor da paleta; pincel, balde, juntar, nova cor |
| 4. Camadas | `src/colors.rs` → `build_layers` | Cores iguais viram uma camada (máscara P&B) |
| 5. Vetorizar | `src/vectorize.rs` → `vectorize_layers` | Corta as margens, fecha furinhos, agrupa pixels em formas e traça contornos em curvas Bézier com [visioncortex](https://github.com/visioncortex/visioncortex) (motor do VTracer) |
| 6. Prévia | `src/worker.rs` → `render_svg` | Renderiza o próprio SVG com `resvg`, mostrando exatamente o que vai pro slicer |
| 7. 3D | `src/mesh.rs` | Triangula cada forma (`earcut`), extruda (tampa, fundo, paredes) e grava STL binário |

## Estrutura do código

```
src/
  lib.rs          núcleo sem interface — reaproveitável e testado
  colors.rs       detecção de cores (k-means), Document editável, camadas
  history.rs      desfazer/refazer (Arc + copy-on-write)
  processing.rs   imagem → máscara preto e branco
  vectorize.rs    máscara → SVG em mm + polígonos
  mesh.rs         polígonos → sólido 3D → STL
  main.rs         ponto de entrada, abre a janela
  app.rs          interface: estado, ações, painéis
  app/canvas.rs   tela de edição: zoom, mover, ferramentas
  app/mat.rs      base de corte: grade em mm, réguas, cotas
  app/widgets.rs  componentes visuais: botões, abas, segmentado, carretéis
  app/theme.rs    paleta "Bancada", fontes (Lato, Quicksand), ícones (Phosphor)
  worker.rs       detecção e vetorização numa thread separada
examples/
  converter.rs    versão linha de comando
```

### Visual: "Bancada"

O app é desenhado como uma bancada de oficina:

- **Base de corte** verde na área de edição, com grade e réguas em **milímetros reais** (ligadas à largura de exportação) e cotas amarelas de largura e altura da peça. As cores marcadas como fundo ficam transparentes e deixam a base aparecer.
- **Alumínio claro** nas barras, **inspetor** à direita com seções separadas por linhas finas.
- **Amarelo de estilete** só para o que importa: ação principal, ferramenta ativa e medidas.
- Cores da peça mostradas como **carretéis de filamento**.
- **Quicksand** (pontas arredondadas, como letra extrudada) em títulos e medidas; **Lato** no texto.

Paleta e fontes em `src/app/theme.rs`; base de corte em `src/app/mat.rs`.

### Bibliotecas

- [`eframe`/`egui`](https://github.com/emilk/egui) — interface gráfica em Rust puro, modo "imediato" (a função `ui` redesenha tudo a cada frame).
- [`image`](https://crates.io/crates/image) — abrir/salvar e manipular imagens.
- [`visioncortex`](https://crates.io/crates/visioncortex) — vetorização.
- [`resvg`](https://crates.io/crates/resvg) — renderizar SVG.
- [`rfd`](https://crates.io/crates/rfd) — diálogos nativos de abrir/salvar.
- [`earcut`](https://crates.io/crates/earcut) — triangulação dos polígonos para o STL.
- [`egui-pop-phosphor`](https://crates.io/crates/egui-pop-phosphor) — fonte de ícones.

## Notas para aprender Rust com este projeto

Conceitos que aparecem no código, em ordem de dificuldade:

- **`struct` + `impl` + `#[derive(...)]`** — `BwParams`, `VectorParams` (`processing.rs`, `vectorize.rs`).
- **`enum` + `match`** — `ThresholdMode`, `CurveMode`; o compilador obriga tratar todos os casos.
- **`Option` / `Result`** — sem `null` nem exceções; veja `let ... else` em `app.rs` e o `match image::open(...)`.
- **Empréstimo (`&` e `&mut`)** — funções recebem `&GrayImage` (só leitura) para não copiar a imagem.
- **`Arc::make_mut` (copy-on-write)** — `app.rs` + `history.rs`: o desfazer guarda versões sem copiar até alguém editar.
- **Submódulos e privacidade** — `app/canvas.rs` enxerga campos privados de `CriadorApp` porque é filho do módulo `app`.
- **Threads com escopo** — `vectorize.rs`: `std::thread::scope` vetoriza cada camada em paralelo, emprestando dados sem `Arc`.
- **Threads e canais** — `worker.rs`: `mpsc::channel` para mandar trabalho e receber resultado; `Arc` para compartilhar a imagem sem copiar.
- **Testes** — blocos `#[cfg(test)]` no fim de cada módulo; rode `cargo test`.

## Ideias para próximas versões

- Gerar **base do chaveiro** (contorno com margem + furo da argola) direto no SVG.
- Exportar **3MF** com as cores já atribuídas (multicor sem configurar no slicer).
- Prévia 3D dentro do app.
- Reordenar camadas manualmente.
- Remover fundo automaticamente.

## Créditos

- Ícones: [Phosphor Icons](https://phosphoricons.com) (MIT), via a crate `egui-pop-phosphor`.
- Fontes: [Lato](https://www.latofonts.com) e [Quicksand](https://fonts.google.com/specimen/Quicksand) (SIL Open Font License) — licenças em `assets/fonts/`.
- Interface: [egui](https://github.com/emilk/egui). Vetorização: [visioncortex](https://github.com/visioncortex/visioncortex).
