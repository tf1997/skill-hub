import { message } from "@tauri-apps/api/dialog";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { AboutPayload } from "../components/dialogs/AboutDialog";
import type { AppUpdateDialogState } from "../components/dialogs/AppUpdateDialog";
import { readError } from "../lib/errors";
import type { UpdateCheckResult } from "../types";

const canUseTauriEvents =
  typeof window !== "undefined" && typeof (window as Window & { __TAURI_IPC__?: unknown }).__TAURI_IPC__ === "function";

export function useAppUpdates() {
  const [about, setAbout] = useState<AboutPayload | null>(null);
  const [appUpdateDialog, setAppUpdateDialog] = useState<AppUpdateDialogState>({
    open: false,
    phase: "checking",
    manual: false
  });
  const checkingAppUpdateRef = useRef(false);

  const openAppUpdateDialog = useCallback((manual = true) => {
    setAppUpdateDialog({
      open: true,
      phase: "checking",
      manual
    });
    void api
      .checkForUpdates()
      .then((result) => {
        setAppUpdateDialog({
          open: true,
          phase: result.available ? "available" : "current",
          result,
          manual
        });
      })
      .catch((err) => {
        setAppUpdateDialog({
          open: true,
          phase: "error",
          error: readError(err),
          manual
        });
      });
  }, []);

  const showAvailableAppUpdate = useCallback((result: UpdateCheckResult, manual = false) => {
    setAppUpdateDialog({
      open: true,
      phase: "available",
      result,
      manual
    });
  }, []);

  const downloadAppUpdate = useCallback(async () => {
    setAppUpdateDialog((current) => ({
      ...current,
      phase: "downloading",
      error: null
    }));
    try {
      const downloaded = await api.downloadUpdate();
      setAppUpdateDialog((current) => ({
        ...current,
        phase: "downloaded",
        downloaded
      }));
    } catch (err) {
      setAppUpdateDialog((current) => ({
        ...current,
        phase: "error",
        error: readError(err)
      }));
    }
  }, []);

  const restartAfterAppUpdate = useCallback(async () => {
    try {
      await api.restartAfterUpdate();
    } catch (err) {
      setAppUpdateDialog((current) => ({
        ...current,
        phase: "error",
        error: readError(err)
      }));
    }
  }, []);

  const closeAppUpdateDialog = useCallback(() => {
    setAppUpdateDialog((current) => ({
      ...current,
      open: false
    }));
    checkingAppUpdateRef.current = false;
  }, []);

  const handleBackgroundAppUpdateAvailable = useCallback(
    async (result: UpdateCheckResult) => {
      if (!result.available || checkingAppUpdateRef.current) return;
      checkingAppUpdateRef.current = true;
      try {
        showAvailableAppUpdate(result, false);
      } catch (err) {
        await message(readError(err), {
          title: "Skill Hub 更新失败",
          type: "error",
          okLabel: "确定"
        });
      } finally {
        checkingAppUpdateRef.current = false;
      }
    },
    [showAvailableAppUpdate]
  );

  useEffect(() => {
    if (!canUseTauriEvents) return;
    let unlistenUpdate: (() => void) | undefined;
    let unlistenAbout: (() => void) | undefined;
    let unlistenAppUpdate: (() => void) | undefined;
    void listen<UpdateCheckResult>("update-available", (event) => {
      void handleBackgroundAppUpdateAvailable(event.payload);
    }).then((fn) => {
      unlistenUpdate = fn;
    });
    void listen<AboutPayload>("show-about", (event) => {
      setAbout(event.payload);
    }).then((fn) => {
      unlistenAbout = fn;
    });
    void listen("open-app-update", () => {
      openAppUpdateDialog(true);
    }).then((fn) => {
      unlistenAppUpdate = fn;
    });
    return () => {
      unlistenUpdate?.();
      unlistenAbout?.();
      unlistenAppUpdate?.();
    };
  }, [handleBackgroundAppUpdateAvailable, openAppUpdateDialog]);

  return {
    about,
    setAbout,
    appUpdateDialog,
    setAppUpdateDialog,
    openAppUpdateDialog,
    downloadAppUpdate,
    restartAfterAppUpdate,
    closeAppUpdateDialog
  };
}
