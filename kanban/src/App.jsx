import React, { useState } from "react";
import Header from "./components/Header";
import Sidebar from "./components/Sidebar";
import Board from "./components/Board";
import IssueDetail from "./components/IssueDetail";
import LoginScreen from "./components/LoginScreen";
import { useIssueStore } from "./store/issues";
import { useAuthStore } from "./store/auth";

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
  const { token, user } = useAuthStore();
  const selectedIssue = useIssueStore((s) => s.selectedIssue);

  const [repo, setRepo] = useState(() => {
    return localStorage.getItem("gh_kanban_repo") || "";
  });

  // If not logged in, show the login screen
  if (!token || !user) {
    return <LoginScreen />;
  }

  function handleRepoChange(newRepo) {
    setRepo(newRepo);
    localStorage.setItem("gh_kanban_repo", newRepo);
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
