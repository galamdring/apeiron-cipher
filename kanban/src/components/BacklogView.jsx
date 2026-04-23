import React, { useState } from "react";

const MODES = [
  { key: "flat", label: "Flat List" },
  { key: "type", label: "Group by Type" },
  { key: "status", label: "Group by Status" },
  { key: "priority", label: "Priority Order" },
  { key: "epic", label: "Epic Tree" },
];

const s = {
  container: {
    flex: 1,
    overflow: "auto",
    padding: 20,
    background: "#0d1117",
    color: "#e6edf3",
  },
  heading: {
    fontSize: 18,
    fontWeight: 700,
    marginBottom: 16,
    color: "#58a6ff",
  },
  modeStrip: {
    display: "flex",
    gap: 8,
    marginBottom: 20,
  },
  modeBtn: {
    padding: "6px 14px",
    borderRadius: 6,
    border: "1px solid #30363d",
    background: "#161b22",
    color: "#8b949e",
    cursor: "pointer",
    fontSize: 13,
    fontWeight: 500,
    transition: "all 0.15s",
  },
  modeBtnActive: {
    background: "#1f6feb",
    color: "#fff",
    borderColor: "#1f6feb",
  },
  placeholder: {
    color: "#8b949e",
    fontSize: 14,
    fontStyle: "italic",
  },
};

export default function BacklogView() {
  const [mode, setMode] = useState("flat");

  return (
    <div style={s.container}>
      <div style={s.heading}>Backlog</div>
      <div style={s.modeStrip}>
        {MODES.map((m) => (
          <button
            key={m.key}
            style={{
              ...s.modeBtn,
              ...(mode === m.key ? s.modeBtnActive : {}),
            }}
            onClick={() => setMode(m.key)}
          >
            {m.label}
          </button>
        ))}
      </div>
      <div style={s.placeholder}>mode: {mode}</div>
    </div>
  );
}
