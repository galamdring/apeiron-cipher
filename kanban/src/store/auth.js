import { create } from "zustand";

const TOKEN_KEY = "gh_kanban_token";
const USER_KEY = "gh_kanban_user";
const REPO_KEY = "gh_kanban_repo";

export const useAuthStore = create((set) => ({
  token: localStorage.getItem(TOKEN_KEY) || null,
  user: (() => {
    try {
      const raw = localStorage.getItem(USER_KEY);
      return raw ? JSON.parse(raw) : null;
    } catch {
      return null;
    }
  })(),

  setAuth(token, user) {
    localStorage.setItem(TOKEN_KEY, token);
    localStorage.setItem(USER_KEY, JSON.stringify(user));
    set({ token, user });
  },

  clearAuth() {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(USER_KEY);
    set({ token: null, user: null });
  },

  // Full sign-out: clears credentials AND the saved repo selection so the
  // next session starts clean. Also used by the API interceptor when the
  // token comes back as invalid / expired.
  signOut() {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(USER_KEY);
    localStorage.removeItem(REPO_KEY);
    set({ token: null, user: null });
  },
}));
