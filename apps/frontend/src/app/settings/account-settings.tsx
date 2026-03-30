"use client";

import { useEffect, useState } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth";
import { getMe, updateEmail, updatePassword } from "@/lib/api";
import { toast } from "@/hooks/use-toast";

export function AccountSettings() {
  const { token, logout, setToken } = useAuth();
  const router = useRouter();

  function handleLogout() {
    logout();
    router.replace("/login");
  }

  // Email state
  const [currentEmail, setCurrentEmail] = useState("");
  const [newEmail, setNewEmail] = useState("");
  const [emailPassword, setEmailPassword] = useState("");
  const [savingEmail, setSavingEmail] = useState(false);

  // Password state
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [savingPassword, setSavingPassword] = useState(false);

  useEffect(() => {
    if (!token) return;
    getMe(token)
      .then((profile) => {
        setCurrentEmail(profile.email);
        setNewEmail(profile.email);
      })
      .catch((e) =>
        toast({ title: "Failed to load profile", description: e.message, variant: "destructive" }),
      );
  }, [token]);

  async function handleEmailSave() {
    if (!token || newEmail === currentEmail) return;
    setSavingEmail(true);
    try {
      const updated = await updateEmail(token, newEmail, emailPassword);
      setCurrentEmail(updated.email);
      setNewEmail(updated.email);
      setEmailPassword("");
      if (updated.access_token) setToken(updated.access_token);
      toast({ title: "Email updated" });
    } catch (e) {
      toast({
        title: "Failed to update email",
        description: e instanceof Error ? e.message : "Unknown error",
        variant: "destructive",
      });
    } finally {
      setSavingEmail(false);
    }
  }

  async function handlePasswordSave() {
    if (!token) return;
    if (newPassword !== confirmPassword) {
      toast({ title: "Passwords do not match", variant: "destructive" });
      return;
    }
    setSavingPassword(true);
    try {
      await updatePassword(token, currentPassword, newPassword);
      setCurrentPassword("");
      setNewPassword("");
      setConfirmPassword("");
      toast({ title: "Password updated" });
    } catch (e) {
      toast({
        title: "Failed to update password",
        description: e instanceof Error ? e.message : "Unknown error",
        variant: "destructive",
      });
    } finally {
      setSavingPassword(false);
    }
  }

  return (
    <div className="flex flex-col gap-6 max-w-lg">
      {/* Email */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Email Address</CardTitle>
          <CardDescription>Update the email address associated with your account</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <label className="text-sm font-medium" htmlFor="email">
              Email
            </label>
            <input
              id="email"
              type="email"
              value={newEmail}
              onChange={(e) => setNewEmail(e.target.value)}
              className="h-9 rounded-md border bg-transparent px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <label className="text-sm font-medium" htmlFor="email-password">
              Confirm Password
            </label>
            <input
              id="email-password"
              type="password"
              value={emailPassword}
              onChange={(e) => setEmailPassword(e.target.value)}
              className="h-9 rounded-md border bg-transparent px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
            />
          </div>
          <Button
            size="sm"
            onClick={handleEmailSave}
            disabled={savingEmail || newEmail === currentEmail || !newEmail || !emailPassword}
          >
            {savingEmail ? "Saving…" : "Save Email"}
          </Button>
        </CardContent>
      </Card>

      <Separator />

      {/* Password */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Change Password</CardTitle>
          <CardDescription>Choose a strong password of at least 8 characters</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <label className="text-sm font-medium" htmlFor="current-password">
              Current Password
            </label>
            <input
              id="current-password"
              type="password"
              value={currentPassword}
              onChange={(e) => setCurrentPassword(e.target.value)}
              className="h-9 rounded-md border bg-transparent px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <label className="text-sm font-medium" htmlFor="new-password">
              New Password
            </label>
            <input
              id="new-password"
              type="password"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              className="h-9 rounded-md border bg-transparent px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <label className="text-sm font-medium" htmlFor="confirm-password">
              Confirm New Password
            </label>
            <input
              id="confirm-password"
              type="password"
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              className="h-9 rounded-md border bg-transparent px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
            />
          </div>
          <Button
            size="sm"
            onClick={handlePasswordSave}
            disabled={savingPassword || !currentPassword || !newPassword || !confirmPassword}
          >
            {savingPassword ? "Saving…" : "Change Password"}
          </Button>
        </CardContent>
      </Card>

      <Separator />

      {/* Logout */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Sign Out</CardTitle>
          <CardDescription>Log out of your account on this device</CardDescription>
        </CardHeader>
        <CardContent>
          <Button variant="destructive" size="sm" onClick={handleLogout}>
            Log Out
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
