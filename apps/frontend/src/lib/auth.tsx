"use client";

import { createContext, useCallback, useContext, useEffect, useState } from "react";
import { setUnauthorizedHandler } from "@/lib/api";

type AuthContextType = {
  token: string | null;
  userEmail: string | null;
  isLoading: boolean;
  login: (email: string, password: string) => Promise<void>;
  register: (email: string, password: string) => Promise<void>;
  logout: () => void;
  setToken: (token: string) => void;
};

const AuthContext = createContext<AuthContextType | null>(null);

function decodeEmail(token: string): string | null {
  try {
    const payload = JSON.parse(atob(token.split(".")[1]));
    return payload.sub ?? null;
  } catch {
    return null;
  }
}

function getTokenExpiry(token: string): number | null {
  try {
    const payload = JSON.parse(atob(token.split(".")[1]));
    return typeof payload.exp === "number" ? payload.exp * 1000 : null;
  } catch {
    return null;
  }
}

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [token, setToken] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const stored = localStorage.getItem("mp_token");
    setToken(stored);
    setIsLoading(false);
  }, []);

  async function login(email: string, password: string) {
    const body = new URLSearchParams({ username: email, password });
    const res = await fetch("/api/auth/token", {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: body.toString(),
    });
    if (!res.ok) {
      const data = await res.json().catch(() => ({}));
      throw new Error(data.detail ?? "Login failed");
    }
    const { access_token } = await res.json();
    localStorage.setItem("mp_token", access_token);
    setToken(access_token);
  }

  async function register(email: string, password: string) {
    const res = await fetch("/api/auth/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email, password }),
    });
    if (!res.ok) {
      const data = await res.json().catch(() => ({}));
      throw new Error(data.detail ?? "Registration failed");
    }
    await login(email, password);
  }

  const logout = useCallback(() => {
    localStorage.removeItem("mp_token");
    setToken(null);
  }, []);

  // Register the 401 handler so any expired-token API response triggers logout.
  useEffect(() => {
    setUnauthorizedHandler(logout);
  }, [logout]);

  // Auto-logout when the JWT reaches its expiry time.
  useEffect(() => {
    if (!token) return;

    const expiry = getTokenExpiry(token);
    if (!expiry) return;

    const msUntilExpiry = expiry - Date.now();
    if (msUntilExpiry <= 0) {
      logout();
      return;
    }

    const timer = setTimeout(logout, msUntilExpiry);
    return () => clearTimeout(timer);
  }, [token, logout]);

  function updateToken(newToken: string) {
    localStorage.setItem("mp_token", newToken);
    setToken(newToken);
  }

  const userEmail = token ? decodeEmail(token) : null;

  return (
    <AuthContext.Provider value={{ token, userEmail, isLoading, login, register, logout, setToken: updateToken }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
