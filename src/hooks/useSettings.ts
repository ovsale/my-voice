import { useCallback, useEffect, useState } from "react";
import type { AppSettings } from "../lib/tauri";
import { tauriAPI } from "../lib/tauri";

export function useSettings() {
	const [settings, setSettings] = useState<AppSettings | null>(null);
	const [isLoading, setIsLoading] = useState(true);

	const refresh = useCallback(async () => {
		const s = await tauriAPI.getSettings();
		setSettings(s);
		setIsLoading(false);
	}, []);

	useEffect(() => {
		refresh();

		let unlisten: (() => void) | undefined;
		tauriAPI.onSettingsChanged(() => {
			refresh();
		}).then((fn) => {
			unlisten = fn;
		});

		return () => {
			unlisten?.();
		};
	}, [refresh]);

	return { settings, isLoading, refresh };
}
