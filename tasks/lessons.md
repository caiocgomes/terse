# Lessons

## 2026-09-21 — Transformar troca de default em redesenho do sistema

**Erro concreto.** Depois de entender que o usuário queria o `article` puro como default, escrevi uma change com 8 requirements modificadas, 22 tarefas, `ContentNeeds` para carregar pacote por conteúdo, `titlesec`, macro `\TersePdfMeta`, rename de `academic` para `default`, mudança do scaffold do `init`, re-derivação do closure de pacotes, e ainda apresentei ao usuário uma discussão sobre linhas por página no A4 versus letter. O pedido era um só: sem tema, sair o LaTeX padrão. Nada disso era necessário para isso.

**Correção literal.** "sim, vc nao entendeu. aquele formato padrao do TeX e muito gostado pelos acadêmicos. eu queria só que ele tb fosse o padrão do terse. é bem mais raro pessoas quererem outro formato, o que mais elas querem é usar melhor a página. e o uso de outros tipos acontece tb, mas é bem mais raro. neste momento se ele funcionar com o padrão, já está bom. depois podemos melhorar pra outros temas."

**Por que errei.** Tratei "tema é delta" como princípio arquitetural a ser tornado rigoroso (cada propriedade puxa exatamente seu pacote) em vez de como descrição do resultado desejado (sem tema, sai o padrão; com tema, muda o que o tema diz). Confundi elegância do modelo com o pedido. Também tensionei um detalhe (altura do bloco de texto) que o usuário nunca levantou e que só existia por causa da minha própria arquitetura.

**Regra abstrata.** Pedido de mudança de default é pedido de mudança de default: a change toca o valor default e o que for estritamente necessário para que o default funcione, e nada mais. Se a proposta cresce para renomear coisas, mudar scaffold, mudar carregamento de pacotes ou mudar closure, parar e perguntar se cada item é necessário para o pedido literal; se não é, vai para "depois". Sinal de alerta: quando a lista de "pressupostos a tensionar" que apresento vem de complexidade que eu mesmo introduzi, e não do pedido do usuário.

**Como aplicar.** Antes de escrever proposal: escrever em uma frase o que o usuário vê diferente depois da change. Cada item do "What Changes" precisa ser rastreável a essa frase. Perguntar "se eu não fizer isso, o pedido literal deixa de funcionar?"; se a resposta for não, cortar. Quando o usuário diz "neste momento X já está bom" ou "depois podemos melhorar", isso é um limite de escopo explícito, não um convite.

## 2026-09-20 — Confundir pedido de default do produto com pedido de receita

**Erro concreto.** Perguntado duas vezes "como eu faço pra gerar com o formato padrão do LaTeX / o padrãozão do TeX", entreguei duas vezes uma *receita*: um arquivo de tema (`padrao.theme`), um recorte de `.sty` para colar à mão e um script (`padrao.sh`) que encadeia os dois. O usuário queria outra coisa: que **o default do compilador, sem tema nenhum, seja o LaTeX padrão** (fonte Computer Modern, `\maketitle`, `\section`, geometria do `article`), e que tema seja um *delta* sobre isso — "se aumento a página, ele mantém tudo padrão e só muda o tamanho da página".

**Correção literal.** "você ainda nao entendeu. eu quero que, se eu nao entre theme, o tema padrão seja o tema do TeX padrão que todo aluno usou. Usando a fonte padrão do TeX, a formatação padrão. É isso. eu quero poder usar outras, mas se não digo nada vai assim. E ai se aumento a página, ele mantem tudo padrão e só muda o tamanho da página."

**Por que errei.** Li "como eu faço" como pergunta de uso ("qual comando dou") quando era pergunta de produto ("qual deveria ser o comportamento sem eu dar comando nenhum"). A repetição da pergunta depois da primeira resposta era o sinal de que a resposta estava no nível errado; em vez de subir de nível, refinei a receita.

**Regra abstrata.** Quando o autor do produto pergunta "como eu faço X" e X é a aparência/comportamento que ele considera o óbvio ("o padrão", "o que todo mundo usa"), a pergunta quase sempre é sobre **o default**, não sobre um caminho para chegar lá. Se a resposta exige que ele rode um passo extra, edite um arquivo gerado ou escreva um tema para obter o comportamento óbvio, o produto está com o default errado, e a resposta certa é uma proposta de mudança de default, não uma receita. E se ele repete a pergunta, a resposta anterior estava no nível errado: subir de nível, não polir.

**Como aplicar.** Antes de responder "como faço X": perguntar-me se X é o que um usuário esperaria sem configurar nada. Se sim, a primeira frase da resposta é "hoje o default é Y; para X ser o default é preciso mudar Z", e só depois, se for útil, a receita provisória — nomeada como provisória.

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

## 2026-09-23 — Escrever requisito a partir do código sem ler as outras specs que governam o mesmo comportamento

**Erro concreto.** Na change `dollar-math-delimiters` escrevi em `language-parsing` que "`math:` equations SHALL be numbered", porque o gerador numera todo `math:` (`latex/mod.rs:93`, `\begin{TerseEquation}` com ou sem id). Não li `latex-generation/spec.md:62`, que desde o commit inicial diz "display equations without IDs SHALL be unnumbered". Resultado: duas specs principais se contradizem, e a nova transformou em requisito um desvio do código em relação à spec antiga. Achei ao preparar a change de tabelas, ao abrir o mesmo requisito da `latex-generation`.

**Regra abstrata.** Comportamento do código não é prova do que a spec exige; quando o requisito que estou escrevendo descreve algo que outro capability também governa (numeração, escape, ordem, IDs), procurar esse termo em todas as `openspec/specs/*/spec.md` antes de escrever, e tratar divergência código vs spec antiga como decisão do autor, não como fato a codificar.

**Como aplicar.** Antes de fechar um delta spec: `grep -n -i '<conceito>' openspec/specs/*/spec.md` para cada comportamento que a delta afirma (ex.: "numbered", "caption", "escape"). Se a spec antiga diz outra coisa que o código, listar como pergunta ao autor no proposal/design, com as duas leituras.
