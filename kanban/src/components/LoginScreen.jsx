import React from "react";
import { redirectToGitHubLogin } from "../api/auth";

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
  err: {
    fontSize: 13,
    color: "#f85149",
    textAlign: "center",
  },
};

export default function LoginScreen({ error }) {
  return (
    <div style={s.wrap}>
      <div style={s.card}>
        <div style={s.title}>GitHub Kanban</div>
        <div style={s.subtitle}>
          Sign in with GitHub to manage your issues on a Kanban board.
        </div>
        {error && (
          <div style={s.err}>Login failed: {decodeURIComponent(error)}</div>
        )}
        <button style={s.btn} onClick={redirectToGitHubLogin}>
          Sign in with GitHub
        </button>
      </div>
    </div>
  );
}
