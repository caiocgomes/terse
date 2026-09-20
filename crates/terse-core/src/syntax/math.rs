//! Closed TeX math subset validator (version 1).
//!
//! Validates payload bytes for `math:` display blocks and inline `\(...\)`
//! math against a documented, versioned allowlist of ordinary and AMS TeX
//! math commands/environments. Rejects I/O, process execution, macro
//! definitions, catcode changes, dynamic command construction, `^^` input
//! encodings, and control bytes, scanning nested control sequences
//! recursively (i.e. inside braces of an otherwise-allowed command) rather
//! than only at brace depth zero. Never invokes a process; this is a pure
//! byte scan over the already-dedented payload.

pub const MATH_SUBSET_VERSION: u32 = 1;

const ALLOWED_COMMANDS: &[&str] = &[
    "frac", "dfrac", "tfrac", "sqrt", "partial", "dot", "ddot", "hat", "bar", "vec", "tilde",
    "overline", "underline", "sum", "prod", "int", "iint", "iiint", "oint", "lim", "infty",
    "cdot", "cdots", "ldots", "vdots", "ddots", "times", "div", "pm", "mp", "leq", "geq", "neq",
    "approx", "equiv", "sim", "simeq", "propto", "in", "notin", "subset", "subseteq", "supset",
    "supseteq", "cup", "cap", "setminus", "emptyset", "varnothing", "forall", "exists", "nabla",
    "alpha", "beta", "gamma", "delta", "epsilon", "varepsilon", "zeta", "eta", "theta",
    "vartheta", "iota", "kappa", "lambda", "mu", "nu", "xi", "pi", "varpi", "rho", "varrho",
    "sigma", "varsigma", "tau", "upsilon", "phi", "varphi", "chi", "psi", "omega", "Gamma",
    "Delta", "Theta", "Lambda", "Xi", "Pi", "Sigma", "Upsilon", "Phi", "Psi", "Omega", "left",
    "right", "begin", "end", "text", "mathrm", "mathbf", "mathit", "mathcal", "mathbb", "mathsf",
    "boldsymbol", "operatorname", "label", "quad", "qquad", "backslash", "langle", "rangle",
    "lvert", "rvert", "lVert", "rVert", "cdotp", "circ", "wedge", "vee", "oplus", "otimes",
    "perp", "parallel", "angle", "triangle", "rightarrow", "leftarrow", "Rightarrow",
    "Leftarrow", "leftrightarrow", "Leftrightarrow", "longrightarrow", "longleftarrow",
    "mapsto", "to", "min", "max", "sup", "inf", "arg", "argmax", "argmin", "log", "ln", "exp",
    "sin", "cos", "tan", "cot", "sec", "csc", "sinh", "cosh", "tanh", "det", "dim", "gcd", "hom",
    "ker", "deg", "binom", "choose", "big", "Big", "bigg", "Bigg", "displaystyle", "textstyle",
    "scriptstyle", "scriptscriptstyle", "limits", "nolimits", "hline", "multicolumn",
    "substack", "bigcup", "bigcap", "bigoplus", "bigotimes", "prime", "star", "ast", "top",
    "bot", "hbar", "ell", "Re", "Im", "aleph", "imath", "jmath", "not",
];

const ALLOWED_ENVIRONMENTS: &[&str] = &[
    "aligned", "matrix", "pmatrix", "bmatrix", "vmatrix", "Vmatrix", "cases", "gathered",
    "split", "array", "smallmatrix",
];

const FORBIDDEN_COMMANDS: &[&str] = &[
    "input", "include", "write", "csname", "endcsname", "def", "edef", "gdef", "xdef", "let",
    "futurelet", "catcode", "openin", "openout", "closein", "closeout", "read", "immediate",
    "special", "expandafter", "noexpand", "the", "string", "meaning", "jobname",
    "InputIfFileExists", "directlua", "luaescapestring", "csstring", "outer", "long", "global",
    "afterassignment", "aftergroup", "everypar", "everymath", "output", "escapechar",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathError {
    UnbalancedBraces { byte_offset: usize },
    UnknownCommand { name: String, byte_offset: usize },
    ForbiddenCommand { name: String, byte_offset: usize },
    UnknownEnvironment { name: String, byte_offset: usize },
    MismatchedEnvironmentEnd {
        expected: Option<String>,
        found: String,
        byte_offset: usize,
    },
    ControlByte { byte_offset: usize },
    EncodedInput { byte_offset: usize },
}

/// Validates `payload` (already structurally dedented) against the closed
/// math subset. Returns `Ok(())` without modifying the payload; accepted
/// bytes are emitted unchanged by the caller.
pub fn validate(payload: &str) -> Result<(), MathError> {
    let bytes = payload.as_bytes();
    let mut i = 0usize;
    let mut brace_depth: i32 = 0;
    let mut env_stack: Vec<String> = Vec::new();

    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x20 && b != b'\n' && b != b'\r' && b != b'\t' {
            return Err(MathError::ControlByte { byte_offset: i });
        }
        match b {
            b'%' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                brace_depth += 1;
                i += 1;
            }
            b'}' => {
                brace_depth -= 1;
                if brace_depth < 0 {
                    return Err(MathError::UnbalancedBraces { byte_offset: i });
                }
                i += 1;
            }
            b'^' if i + 1 < bytes.len() && bytes[i + 1] == b'^' => {
                return Err(MathError::EncodedInput { byte_offset: i });
            }
            b'\\' => {
                let start = i;
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                let c = bytes[i];
                if c.is_ascii_alphabetic() {
                    let name_start = i;
                    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let name = &payload[name_start..i];
                    if FORBIDDEN_COMMANDS.contains(&name) {
                        return Err(MathError::ForbiddenCommand {
                            name: name.to_string(),
                            byte_offset: start,
                        });
                    }
                    if name == "begin" || name == "end" {
                        let (env_name, next) = read_environment_name(payload, bytes, i)
                            .ok_or(MathError::UnknownCommand {
                                name: name.to_string(),
                                byte_offset: start,
                            })?;
                        if !ALLOWED_ENVIRONMENTS.contains(&env_name) {
                            return Err(MathError::UnknownEnvironment {
                                name: env_name.to_string(),
                                byte_offset: i,
                            });
                        }
                        if name == "begin" {
                            env_stack.push(env_name.to_string());
                        } else {
                            match env_stack.pop() {
                                Some(open) if open == env_name => {}
                                other => {
                                    return Err(MathError::MismatchedEnvironmentEnd {
                                        expected: other,
                                        found: env_name.to_string(),
                                        byte_offset: i,
                                    })
                                }
                            }
                        }
                        i = next;
                        continue;
                    }
                    if !ALLOWED_COMMANDS.contains(&name) {
                        return Err(MathError::UnknownCommand {
                            name: name.to_string(),
                            byte_offset: start,
                        });
                    }
                } else {
                    // A single non-alphabetic escaped character (`\%`,
                    // `\\`, `\{`, `\}`, `\ `, ...) is always an ordinary
                    // literal escape, never a control sequence name.
                    let ch_len = payload[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                    i += ch_len;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    if brace_depth != 0 || !env_stack.is_empty() {
        return Err(MathError::UnbalancedBraces {
            byte_offset: bytes.len(),
        });
    }
    Ok(())
}

fn read_environment_name<'a>(payload: &'a str, bytes: &[u8], mut j: usize) -> Option<(&'a str, usize)> {
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }
    if j >= bytes.len() || bytes[j] != b'{' {
        return None;
    }
    j += 1;
    let name_start = j;
    while j < bytes.len() && bytes[j] != b'}' {
        j += 1;
    }
    if j >= bytes.len() {
        return None;
    }
    Some((&payload[name_start..j], j + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accepts_familiar_notation() {
        assert!(validate(r"\frac{\partial \dot V}{\partial H} > 0.").is_ok());
        assert!(validate(r"\begin{aligned} a &= b \\ c &= d \end{aligned}").is_ok());
        assert!(validate(r"\begin{cases} 1 & x > 0 \\ 0 & x \leq 0 \end{cases}").is_ok());
    }

    #[test]
    fn test_rejects_unbalanced_braces() {
        assert!(matches!(
            validate(r"\frac{a}{b"),
            Err(MathError::UnbalancedBraces { .. })
        ));
    }

    #[test]
    fn test_rejects_unmatched_environment_end() {
        assert!(matches!(
            validate(r"\begin{aligned} a \end{matrix}"),
            Err(MathError::MismatchedEnvironmentEnd { .. })
        ));
    }

    #[test]
    fn test_rejects_execution_and_encoding() {
        assert!(matches!(
            validate(r"\input{evil}"),
            Err(MathError::ForbiddenCommand { .. })
        ));
        assert!(matches!(
            validate(r"\write18{rm -rf /}"),
            Err(MathError::ForbiddenCommand { .. })
        ));
        assert!(matches!(
            validate(r"\csname foo\endcsname"),
            Err(MathError::ForbiddenCommand { .. })
        ));
        assert!(matches!(
            validate("^^49"),
            Err(MathError::EncodedInput { .. })
        ));
        assert!(matches!(
            validate(r"\frac{\input{x}}{2}"),
            Err(MathError::ForbiddenCommand { .. })
        ));
    }
}
