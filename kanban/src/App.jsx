import React, { useState } from "react";
import Header from "./components/Header";
import Sidebar from "./components/Sidebar";
import Board from "./components/Board";
import IssueDetail from "./components/IssueDetail";
import { useIssueStore } from "./store/issues";

const styles = {
  app: {
    display: "flex",
    flexDirection: "column",
    height: "100vh",
    overflow: "hidden",
  },
  body: {
    display: "flex",
    flex: 1,
    overflow: "hidden",
  },
};

export default function App() {
  const [repo, setRepo] = useState(() => {
    return localStorage.getItem("gh_kanban_repo") || "";
  });
  const [token, setToken] = useState(() => {
    return localStorage.getItem("gh_kanban_token") || "";
  });
  const selectedIssue = useIssueStore((s) => s.selectedIssue);

  function handleRepoChange(newRepo, newToken) {
    setRepo(newRepo);
    setToken(newToken);
    localStorage.setItem("gh_kanban_repo", newRepo);
    localStorage.setItem("gh_kanban_token", newToken);
  }

  return (
    <div style={styles.app}>
      <Header repo={repo} onRepoChange={handleRepoChange} />
      <div style={styles.body}>
        <Sidebar />
        <Board repo={repo} token={token} />
      </div>
      {selectedIssue && <IssueDetail repo={repo} token={token} />}
    </div>
  );
}
