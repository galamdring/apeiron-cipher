import React, { useEffect, useState } from "react";
import Header from "./components/Header";
import Sidebar from "./components/Sidebar";
import Board from "./components/Board";
import IssueDetail from "./components/IssueDetail";
import LoginScreen from "./components/LoginScreen";
import { checkSession, parseErrorFromHash } from "./api/auth";
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
  const { user, setUser, sessionExpired } = useAuthStore();
  const selectedIssue = useIssueStore((s) => s.selectedIssue);
  const [bootstrapping, setBootstrapping] = useState(true);
  const [hashError, setHashError] = useState(null);

  const [repo, setRepo] = useState(() => {
    return localStorage.getItem("gh_kanban_repo") || "";
  });

  useEffect(() => {
    async function bootstrap() {
      // Check for an error in the hash (e.g. OAuth failure redirect).
      const err = parseErrorFromHash();
      if (err) {
        setHashError(err);
        window.history.replaceState(null, "", window.location.pathname);
        setBootstrapping(false);
        return;
      }

      // Clean any leftover hash from a successful OAuth redirect.
      if (window.location.hash) {
        window.history.replaceState(null, "", window.location.pathname);
      }

      // Check if we have a valid server-side session.
      const sessionUser = await checkSession();
      if (sessionUser) {
        setUser(sessionUser);
      }

      setBootstrapping(false);
    }

    bootstrap();
  }, []);

  if (bootstrapping) {
    return <div style={styles.loading}>Loading…</div>;
  }

  if (!user) {
    const displayError = hashError || (sessionExpired ? "Session expired — please sign in again." : null);
    return <LoginScreen error={displayError} config={config} />;
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
        <Board repo={repo} />
      </div>
      {selectedIssue && <IssueDetail repo={repo} />}
    </div>
  );
}
