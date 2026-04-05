import React, { useState } from "react";
import { requestDeviceCode, pollForToken, fetchAuthenticatedUser } from "../api/auth";
import { useAuthStore } from "../store/auth";

const s = {
  wrap: {
    height: "100vh",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    background: "#0d1117",
  },
  card: {
    background: "#161b22",
    border: "1px solid #30363d",
    borderRadius: 12,
    padding: "40px 48px",
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    gap: 20,
    minWidth: 340,
  },
  title: {
    fontSize: 22,
    fontWeight: 700,
    color: "#58a6ff",
  },
  subtitle: {
    fontSize: 14,
    color: "#8b949e",
    textAlign: "center",
    lineHeight: 1.5,
  },
  btn: {
    background: "#238636",
    color: "#fff",
    border: "none",
    borderRadius: 6,
    padding: "10px 28px",
    cursor: "pointer",
    fontWeight: 600,
    fontSize: 15,
    width: "100%",
  },
  codeBox: {
    background: "#0d1117",
    border: "1px solid #388bfd",
    borderRadius: 8,
    padding: "14px 24px",
    fontSize: 28,
    fontWeight: 700,
    letterSpacing: 6,
    color: "#58a6ff",
    textAlign: "center",
    fontFamily: "monospace",
  },
  link: {
    color: "#58a6ff",
    fontSize: 14,
  },
  status: {
    fontSize: 13,
    color: "#8b949e",
    textAlign: "center",
  },
  err: {
    fontSize: 13,
    color: "#f85149",
    textAlign: "center",
  },
  copyBtn: {
    background: "none",
    border: "1px solid #30363d",
    borderRadius: 6,
    color: "#8b949e",
    fontSize: 12,
    padding: "4px 12px",
    cursor: "pointer",
  },
};

export default function LoginScreen() {
  const setAuth = useAuthStore((s) => s.setAuth);
  const [step, setStep] = useState("idle"); // idle | waiting | polling | error
  const [userCode, setUserCode] = useState("");
  const [verificationUri, setVerificationUri] = useState("");
  const [error, setError] = useState(null);
  const [copied, setCopied] = useState(false);

  async function handleLogin() {
    setError(null);
    setStep("waiting");
    try {
      const { device_code, user_code, verification_uri, interval } =
        await requestDeviceCode();
      setUserCode(user_code);
      setVerificationUri(verification_uri);
      setStep("polling");

      const token = await pollForToken(device_code, interval);
      const user = await fetchAuthenticatedUser(token);
      setAuth(token, user);
    } catch (e) {
      setError(e.message || "Login failed");
      setStep("error");
    }
  }

  function handleCopy() {
    navigator.clipboard.writeText(userCode).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }

  return (
    <div style={s.wrap}>
      <div style={s.card}>
        <div style={s.title}>GitHub Kanban</div>

        {step === "idle" && (
          <>
            <div style={s.subtitle}>
              Sign in with GitHub to manage your issues on a Kanban board.
            </div>
            <button style={s.btn} onClick={handleLogin}>
              Sign in with GitHub
            </button>
          </>
        )}

        {step === "waiting" && (
          <div style={s.status}>Requesting login code…</div>
        )}

        {step === "polling" && (
          <>
            <div style={s.subtitle}>
              Copy this code, then click the link below and paste it in.
            </div>
            <div style={s.codeBox}>{userCode}</div>
            <button style={s.copyBtn} onClick={handleCopy}>
              {copied ? "Copied!" : "Copy code"}
            </button>
            <a
              href={verificationUri}
              target="_blank"
              rel="noreferrer"
              style={s.link}
            >
              {verificationUri} ↗
            </a>
            <div style={s.status}>Waiting for you to approve…</div>
          </>
        )}

        {step === "error" && (
          <>
            <div style={s.err}>{error}</div>
            <button style={s.btn} onClick={handleLogin}>
              Try again
            </button>
          </>
        )}
      </div>
    </div>
  );
}
