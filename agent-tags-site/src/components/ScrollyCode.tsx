import { useState, useEffect, useRef } from "react";

const CODE = [
  "/**",
  " * @agents(auth-module)",
  " * OAuth PKCE flow for third-party providers.",
  " * Uses refresh tokens in httpOnly cookies.",
  " * Related: src/auth/guard.ts#token-validation",
  " * Related: src/types/auth.d.ts",
  " * Don\u2019t add session logic here \u2014 see session-manager.ts",
  " */",
  "",
  "import { verify, sign } from 'jsonwebtoken';",
  "import { getCookie, setCookie } from '../utils/cookies';",
  "import type { AuthToken, User } from '../types/auth';",
  "",
  "// @agents(token-check): Must validate before refresh, not after.",
  "// Related: src/auth/guard.ts#token-validation",
  "export function validateToken(token: string): boolean {",
  "  try {",
  "    const decoded = verify(token, process.env.JWT_SECRET);",
  "    return decoded.exp > Date.now() / 1000;",
  "  } catch {",
  "    return false;",
  "  }",
  "}",
  "",
  "// @agents(refresh-flow, start): Handles token refresh cycle.",
  "// Warning: Never cache refresh tokens in memory.",
  "// Related: src/auth/session-manager.ts",
  "export async function refreshToken(req: Request): Promise<AuthToken> {",
  "  const current = getCookie(req, 'refresh_token');",
  "  if (!current || !validateToken(current)) {",
  "    throw new AuthError('Invalid refresh token');",
  "  }",
  "",
  "  const user = await lookupUser(current);",
  "  const newAccess = sign({ sub: user.id }, process.env.JWT_SECRET, {",
  "    expiresIn: '15m',",
  "  });",
  "",
  "  const newRefresh = sign({ sub: user.id }, process.env.REFRESH_SECRET, {",
  "    expiresIn: '7d',",
  "  });",
  "",
  "  setCookie(req, 'refresh_token', newRefresh, { httpOnly: true });",
  "  return { accessToken: newAccess, expiresIn: 900 };",
  "}",
  "// @agents(refresh-flow, end)",
];

interface Step {
  title: string;
  desc: string;
  focus: number[];
  highlight: number[];
}

const STEPS: Step[] = [
  {
    title: "File header",
    desc: "The @agents header sits in the first 30 lines and describes the whole file \u2014 what it does, what it touches, and what to avoid. Any agent or human reads this before changing a line.",
    focus: [1, 2, 3, 4, 5, 6, 7, 8],
    highlight: [2],
  },
  {
    title: "Relationship graph",
    desc: "Related: fields link to other files by path. These aren\u2019t just comments \u2014 the CLI validates every reference. Rename guard.ts and your pre-commit hook blocks the commit.",
    focus: [5, 6],
    highlight: [5, 6],
  },
  {
    title: "Constraints",
    desc: "Lines starting with Don\u2019t, Warning:, or Avoid: are parsed as constraints. An agent editing this file knows not to add session logic here without ever opening session-manager.ts.",
    focus: [7],
    highlight: [7],
  },
  {
    title: "Inline tags",
    desc: "@agents(token-check) creates a named anchor at a specific code location. Other files can reference it with token.ts#token-check \u2014 a stable, validated pointer that survives refactors.",
    focus: [14, 15, 16, 17, 18, 19, 20, 21, 22],
    highlight: [14],
  },
  {
    title: "Range markers",
    desc: "start and end markers scope a region. Staleness detection applies only to code between them \u2014 so unrelated changes elsewhere in the file don\u2019t trigger false warnings.",
    focus: [24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44],
    highlight: [24, 44],
  },
  {
    title: "The full picture",
    desc: "Run git agent-tags context --for src/auth/token.ts and an agent gets everything \u2014 purpose, constraints, relationships, scoped regions \u2014 without reading dozens of files.",
    focus: [],
    highlight: [],
  },
];

function tokenize(text: string): string {
  let s = text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  s = s.replace(/(\/\/.*$)/gm, '<span class="tok-comment">$1</span>');
  s = s.replace(/(\*[^/].*$)/gm, '<span class="tok-comment">$1</span>');
  s = s.replace(/(\/\*\*)/g, '<span class="tok-comment">$1</span>');
  s = s.replace(/( \*\/)/g, '<span class="tok-comment">$1</span>');
  s = s.replace(
    /\b(import|export|from|const|try|catch|return|throw|if|async|new|type|function)\b/g,
    '<span class="tok-keyword">$1</span>'
  );
  s = s.replace(/('(?:[^'\\]|\\.)*')/g, '<span class="tok-string">$1</span>');
  s = s.replace(/\b(true|false|null|undefined|\d+)\b/g, '<span class="tok-number">$1</span>');
  return s;
}

export default function ScrollyCode() {
  const [active, setActive] = useState(0);
  const stepRefs = useRef<(HTMLDivElement | null)[]>([]);
  const codeAreaRef = useRef<HTMLDivElement>(null);

  // IntersectionObserver — page scroll drives step detection
  useEffect(() => {
    const observers: IntersectionObserver[] = [];

    stepRefs.current.forEach((el, i) => {
      if (!el) return;
      const observer = new IntersectionObserver(
        ([entry]) => {
          if (entry.isIntersecting) {
            setActive(i);
          }
        },
        { rootMargin: "-35% 0px -35% 0px", threshold: 0 }
      );
      observer.observe(el);
      observers.push(observer);
    });

    return () => observers.forEach((o) => o.disconnect());
  }, []);

  // Programmatically scroll code panel (no user scrolling allowed)
  useEffect(() => {
    const step = STEPS[active];
    if (!step || step.focus.length === 0 || !codeAreaRef.current) return;
    const target = codeAreaRef.current.children[step.focus[0] - 1] as HTMLElement;
    if (target) {
      const c = codeAreaRef.current;
      c.scrollTo({
        top: Math.max(0, target.offsetTop - c.clientHeight / 3),
        behavior: "smooth",
      });
    }
  }, [active]);

  const step = STEPS[active];
  const hasFocus = step.focus.length > 0;

  return (
    <>
      <style>{`
        .scrolly-root {
          display: flex;
          position: relative;
        }

        /* Code panel — sticky, not user-scrollable */
        .scrolly-code-panel {
          flex: 0 0 58%;
          position: sticky;
          top: var(--sl-nav-height, 3.5rem);
          height: calc(100vh - var(--sl-nav-height, 3.5rem));
          display: flex;
          flex-direction: column;
          background: #0a0c14;
          border-right: 1px solid #1a1d2a;
        }

        .scrolly-code-area {
          flex: 1;
          overflow-y: scroll;
          padding: 20px 0;
          font-family: 'JetBrains Mono', 'SF Mono', Menlo, monospace;
          font-size: 12.5px;
          line-height: 1.8;
          pointer-events: none;
          scrollbar-width: none;
        }
        .scrolly-code-area::-webkit-scrollbar { display: none; }

        /* Code lines */
        .code-line {
          display: flex;
          padding: 0 20px;
          border-left: 2px solid transparent;
          transition: all 0.5s cubic-bezier(0.4, 0, 0.2, 1);
        }
        .code-line--highlighted {
          background: rgba(52, 211, 153, 0.1);
          border-left-color: #34d399;
        }
        .code-line--focused {
          background: rgba(52, 211, 153, 0.03);
        }
        .code-line--dimmed {
          opacity: 0.15;
        }
        .line-number {
          flex: 0 0 32px;
          color: #1e2235;
          font-size: 11px;
          text-align: right;
          padding-right: 16px;
          user-select: none;
          font-variant-numeric: tabular-nums;
          transition: color 0.5s ease;
        }
        .code-line--highlighted .line-number {
          color: #34d399;
        }
        .line-content {
          flex: 1;
          white-space: pre;
          color: #c0c5d4;
        }

        /* Syntax tokens */
        .tok-comment { color: #4b5563; }
        .tok-keyword { color: #c084fc; }
        .tok-string { color: #34d399; }
        .tok-number { color: #fbbf24; }

        /* Steps panel — flows with page scroll */
        .scrolly-steps {
          flex: 0 0 42%;
          padding: 0 clamp(28px, 3vw, 56px);
        }

        .step-block {
          min-height: 80vh;
          display: flex;
          flex-direction: column;
          justify-content: center;
        }
        .step-block:first-child {
          min-height: 60vh;
          padding-top: 15vh;
        }
        .step-block:last-child {
          min-height: 90vh;
        }

        .step-label {
          font-family: 'JetBrains Mono', monospace;
          font-size: 11px;
          font-weight: 600;
          letter-spacing: 0.1em;
          text-transform: uppercase;
          margin-bottom: 12px;
          transition: color 0.4s ease;
        }
        .step-label--active { color: #34d399; }
        .step-label--inactive { color: #2d3148; }

        .step-desc {
          font-family: 'Instrument Sans', system-ui, sans-serif;
          font-size: 15px;
          line-height: 1.75;
          max-width: 340px;
          margin: 0;
          transition: color 0.5s ease;
        }
        .step-desc--active { color: #9ca3af; }
        .step-desc--inactive { color: #2d3148; }

        /* Responsive */
        @media (max-width: 768px) {
          .scrolly-root {
            flex-direction: column;
          }
          .scrolly-code-panel {
            flex: none;
            position: sticky;
            top: var(--sl-nav-height, 3.5rem);
            height: 40vh;
            border-right: none;
            border-bottom: 1px solid #1a1d2a;
          }
          .scrolly-steps {
            flex: none;
            padding: 0 20px;
          }
          .step-block {
            min-height: 50vh;
          }
          .step-block:first-child {
            padding-top: 2rem;
            min-height: 40vh;
          }
        }
      `}</style>

      <div className="scrolly-root">
        <div className="scrolly-code-panel">
          <div ref={codeAreaRef} className="scrolly-code-area">
            {CODE.map((line, i) => {
              const ln = i + 1;
              const focused = hasFocus && step.focus.includes(ln);
              const highlighted = hasFocus && step.highlight.includes(ln);
              const dimmed = hasFocus && !focused && !highlighted;

              let cls = "code-line";
              if (highlighted) cls += " code-line--highlighted";
              else if (focused) cls += " code-line--focused";
              if (dimmed) cls += " code-line--dimmed";

              return (
                <div key={i} className={cls}>
                  <span className="line-number">{ln}</span>
                  <span
                    className="line-content"
                    dangerouslySetInnerHTML={{ __html: tokenize(line) }}
                  />
                </div>
              );
            })}
          </div>
        </div>

        <div className="scrolly-steps">
          {STEPS.map((s, i) => (
            <div
              key={i}
              ref={(el) => { stepRefs.current[i] = el; }}
              className="step-block"
            >
              <p className={`step-label ${active === i ? "step-label--active" : "step-label--inactive"}`}>
                {s.title}
              </p>
              <p className={`step-desc ${active === i ? "step-desc--active" : "step-desc--inactive"}`}>
                {s.desc}
              </p>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
