import { create } from "zustand";

const USER_KEY = "gh_kanban_user";
const REPO_KEY = "gh_kanban_repo";

export const useAuthStore = create((set) => ({
  // With httpOnly cookies the token never touches JS. Auth state is just
  // "do we have a user profile from /api/me".
  user: (() => {
    try {
      const raw = localStorage.getItem(USER_KEY);
      return raw ? JSON.parse(raw) : null;
    } catch {
      return null;
    }
  })(),

  setUser(user) {
    localStorage.setItem(USER_KEY, JSON.stringify(user));
    set({ user });
  },

  clearAuth() {
    localStorage.removeItem(USER_KEY);
    set({ user: null });
  },

  // Full sign-out: clears profile AND the saved repo selection so the
  // next session starts clean. Also used by the API interceptor when the
  // session comes back as invalid / expired.
  signOut() {
    localStorage.removeItem(USER_KEY);
    localStorage.removeItem(REPO_KEY);
    set({ user: null });
  },
}));
