import React, { useEffect, useState } from "react";
import Header from "./components/Header";
import Sidebar from "./components/Sidebar";
import Board from "./components/Board";
import IssueDetail from "./components/IssueDetail";
import LoginScreen from "./components/LoginScreen";
import { parseTokenFromHash, fetchAuthenticatedUser } from "./api/auth";
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
  loading: {
    height: "100vh",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    background: "#0d1117",
    color: "#8b949e",
    fontSize: 14,
  },
};

export default function App({ config }) {
  const { token, user, setAuth } = useAuthStore();
  const selectedIssue = useIssueStore((s) => s.selectedIssue);
  const [bootstrapping, setBootstrapping] = useState(true);
  const [hashError, setHashError] = useState(null);

  const [repo, setRepo] = useState(() => {
    return localStorage.getItem("gh_kanban_repo") || "";
  });

  useEffect(() => {
    async function handleCallback() {
      const { token: hashToken, error: hashErr } = parseTokenFromHash();

      if (hashErr) {
        setHashError(hashErr);
        window.history.replaceState(null, "", window.location.pathname);
        setBootstrapping(false);
        return;
      }

      if (hashToken) {
        try {
          const userData = await fetchAuthenticatedUser(hashToken);
          setAuth(hashToken, userData);
        } catch (e) {
          setHashError("Failed to fetch user profile after login");
        }
        // Clean the hash so the token doesn't sit in the address bar
        window.history.replaceState(null, "", window.location.pathname);
        setBootstrapping(false);
        return;
      }

      // No hash — normal load, existing session (if any) is already in the store
      setBootstrapping(false);
    }

    handleCallback();
  }, []);

  if (bootstrapping) {
    return <div style={styles.loading}>Loading…</div>;
  }

  if (!token || !user) {
    return <LoginScreen error={hashError} config={config} />;
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
