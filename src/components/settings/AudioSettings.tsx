import { Switch, Tooltip } from "@mantine/core";
import { useCallback, useEffect, useState } from "react";
import { useMutation } from "../../hooks/useMutation";
import { useSettings } from "../../hooks/useSettings";
import { tauriAPI } from "../../lib/tauri";
import { DeviceSelector } from "../DeviceSelector";
import { StatusIndicator } from "./StatusIndicator";

export function AudioSettings() {
	const { settings, isLoading } = useSettings();
	const [isAudioMuteSupported, setIsAudioMuteSupported] = useState<
		boolean | undefined
	>(undefined);

	useEffect(() => {
		tauriAPI.isAudioMuteSupported().then(setIsAudioMuteSupported);
	}, []);

	const updateSoundEnabled = useMutation(
		useCallback((enabled: boolean) => tauriAPI.updateSoundEnabled(enabled), []),
	);
	const updateAutoMuteAudio = useMutation(
		useCallback(
			(enabled: boolean) => tauriAPI.updateAutoMuteAudio(enabled),
			[],
		),
	);

	return (
		<div className="settings-section animate-in animate-in-delay-2">
			<h3 className="settings-section-title">Audio</h3>
			<div className="settings-card">
				<DeviceSelector />
				<div className="settings-row" style={{ marginTop: 16 }}>
					<div>
						<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
							<p className="settings-label" style={{ margin: 0 }}>
								Sound feedback
							</p>
							<StatusIndicator status={updateSoundEnabled.status} />
						</div>
						<p className="settings-description">
							Play sounds when recording starts and stops
						</p>
					</div>
					<Switch
						checked={settings?.sound_enabled ?? true}
						onChange={(event) =>
							updateSoundEnabled.mutate(event.currentTarget.checked)
						}
						disabled={isLoading}
						color="gray"
						size="md"
					/>
				</div>
				<div className="settings-row" style={{ marginTop: 16 }}>
					<div>
						<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
							<p className="settings-label" style={{ margin: 0 }}>
								Mute audio during recording
							</p>
							<StatusIndicator status={updateAutoMuteAudio.status} />
						</div>
						<p className="settings-description">
							Automatically mute system audio while dictating
						</p>
					</div>
					<Tooltip
						label="Not supported on this platform"
						disabled={isAudioMuteSupported !== false}
						withArrow
					>
						<Switch
							checked={settings?.auto_mute_audio ?? false}
							onChange={(event) =>
								updateAutoMuteAudio.mutate(event.currentTarget.checked)
							}
							disabled={isLoading || isAudioMuteSupported === false}
							color="gray"
							size="md"
						/>
					</Tooltip>
				</div>
			</div>
		</div>
	);
}
