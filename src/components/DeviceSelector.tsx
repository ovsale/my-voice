import { Select } from "@mantine/core";
import { useCallback, useEffect, useState } from "react";
import { useMutation } from "../hooks/useMutation";
import { useSettings } from "../hooks/useSettings";
import type { AudioDeviceInfo } from "../lib/tauri";
import { tauriAPI } from "../lib/tauri";
import { StatusIndicator } from "./settings/StatusIndicator";

/** Interval for polling audio devices since native API lacks change events */
const DEVICE_REFRESH_INTERVAL_MS = 5000;

export function DeviceSelector() {
	const { settings, isLoading: settingsLoading } = useSettings();
	const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
	const [isLoading, setIsLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);

	const updateSelectedMic = useMutation(
		useCallback((micId: string | null) => tauriAPI.updateSelectedMic(micId), []),
	);

	useEffect(() => {
		async function loadDevices() {
			try {
				const nativeDevices = await tauriAPI.listNativeAudioDevices();
				setDevices(nativeDevices);
				setError(null);
			} catch (err) {
				setError("Could not access microphones.");
				console.error("Failed to enumerate devices:", err);
			} finally {
				setIsLoading(false);
			}
		}

		loadDevices();

		const intervalId = setInterval(loadDevices, DEVICE_REFRESH_INTERVAL_MS);

		return () => {
			clearInterval(intervalId);
		};
	}, []);

	const handleChange = (value: string | null) => {
		const micId = value === "" || value === "default" ? null : value;
		updateSelectedMic.mutate(micId);
	};

	if (isLoading || settingsLoading) {
		return (
			<div>
				<p className="settings-label">Microphone</p>
				<p className="settings-description">Loading microphones...</p>
			</div>
		);
	}

	if (error) {
		return (
			<div>
				<p className="settings-label">Microphone</p>
				<p className="settings-description" style={{ color: "#ef4444" }}>
					{error}
				</p>
			</div>
		);
	}

	const selectData = [
		{ value: "default", label: "System Default" },
		...devices.map((device) => ({
			value: device.id,
			label: device.name,
		})),
	];

	return (
		<div>
			<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
				<span className="settings-label">Microphone</span>
				<StatusIndicator status={updateSelectedMic.status} />
			</div>
			<Select
				label={null}
				description="Select which microphone to use for dictation"
				data={selectData}
				value={settings?.selected_mic_id ?? "default"}
				onChange={handleChange}
				allowDeselect={false}
				className="device-selector"
				aria-label="Select microphone device"
			/>
		</div>
	);
}
