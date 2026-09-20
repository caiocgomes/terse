# Lessons

## 2026-09-20 — Afirmar limitação técnica em design.md sem ler o código

**Erro concreto.** Ao escrever `openspec/changes/close-verification-gaps/design.md`, registrei como trade-off aceito: "in two-column mode a wide-role figure fills its column rather than spanning the page, because spanning requires the body to emit `figure*`, which the theme cannot influence". Não verifiquei. O corpo já emite `figure*` para o papel `wide` (`crates/terse-core/src/latex/mod.rs:106`: `let env = if is_wide { "figure*" } else { "figure" }`), então figuras largas spanam normalmente e a limitação não existe.

**Correção.** Veio do agente que implementou o grupo 1: "Contradiction with design.md: it documents 'wide figures fill their column rather than spanning the page' as an accepted two-column limitation. That's wrong — `latex/mod.rs:106` already emits `figure*` for the wide role, so they do span."

**Agravante.** O erro saiu na mesma sessão em que eu tinha acabado de entregar um relatório de verificação cuja tese central é que afirmação sem evidência não vale, e cujo achado principal foi justamente propriedade declarada que não produz efeito. Apliquei ao código um rigor que não apliquei ao meu próprio documento.

**Regra abstrata.** Documento de design não é espaço de especulação. Toda frase sobre comportamento *atual* do sistema ("hoje X não acontece", "isso é limitação conhecida", "Y exigiria Z que não fazemos") é afirmação factual e exige a mesma evidência que uma finding de verificação: o grep ou a leitura do trecho, antes de escrever. O risco específico de errar para o lado pessimista é pior do que parece — uma limitação falsa vira trade-off aceito, e um implementador pode remover ou não construir algo que já funciona.

**Como aplicar.** Antes de escrever em design/spec/proposal qualquer cláusula do tipo "não suportado", "limitação aceita", "exigiria mudar W": localizar o mecanismo no código e citar `arquivo:linha` na própria frase, ou reescrever como pergunta aberta ("verificar se o corpo já emite `figure*`") em vez de asserção. Vale também para prosa em português que descreve o sistema ao usuário, não só para artefatos OpenSpec.

## 2026-09-20 — Repetir fato de ambiente capturado no início da sessão sem reconferir

**Erro concreto.** Afirmei várias vezes, ao longo de horas, que o projeto `~/Dev/terse` não tem git ("Repo has no git", "a CI nunca rodou porque não há git nem remoto"), e escrevi isso na memória do projeto, no plano e nas anotações de task. No fim da sessão o repositório existia, com commit `e282cf7 "Initial commit"` e mudanças não commitadas. O `README.md:90` também afirma ao leitor "This repository does not itself use git (there is no `.git` directory here)", agora falso num projeto que vai a público.

**Correção.** Veio do agente do grupo 4: "A git repo does exist (branch `main`); I made no commits, as none were requested."

**Causa.** O banner de ambiente no início da sessão dizia "Is a git repository: false", o que era verdade naquele momento. O erro não foi a leitura inicial: foi tratar um fato volátil como permanente e continuar repetindo-o sem reconferir, inclusive ao gravá-lo em artefatos duráveis.

**Regra abstrata.** Fato de ambiente lido no início da sessão (existe git, qual toolchain, quais ferramentas estão no PATH, qual versão está instalada) tem validade curta numa sessão longa, porque o usuário e os próprios agentes mudam o sistema enquanto se trabalha. Antes de escrever esse tipo de fato em README, memória, spec ou plano — qualquer coisa que sobreviva à sessão — reconferir com um comando, não com a lembrança.

**Recorrência (mesma raiz, terceira vez no dia).** Registrei no `proposal.md` da change `close-verification-gaps` que `semantic::projection::project()` "não tem chamador fora de teste: a projeção invariante por tema é testada mas nunca computada num build real, o que enfraquece o gate A". O fato é verdadeiro — verifiquei por grep — mas a caracterização é falsa: a requirement "Theme-invariant semantic projection" diz que o compilador **define** a projeção e que "a digest SHALL support comparison", e o cenário põe a comparação no momento em que os planos de build são gerados. Projeção calculada só em teste é conforme; é um oráculo de comparação, não um entregável. Contrasta com `source_map.rs`, cuja saída `paper.map.json` a spec nomeia entre os entregáveis, e com `\TerseLogo`, que tem de aparecer na página — esses dois eram defeitos de verdade. **Antes de chamar algo de defeito, ler a cláusula que o governa**; "existe e não é chamado em produção" só é defeito se algo exigir que seja.

**Como aplicar.** Rodar a checagem barata na hora de escrever (`git rev-parse --is-inside-work-tree`, `which <tool>`, `<tool> --version`) e, quando o fato for a base de uma recomendação ao usuário ("a CI nunca rodou", "você precisa instalar X"), reconferir mesmo que eu já tenha verificado antes na mesma sessão. O caso da atualização do TeX Live 2025→2026 no meio desta sessão é o precedente: o ambiente mudou debaixo do trabalho e quase virou diagnóstico errado.
