import { Slider, Switch, Text, Tooltip } from "@mantine/core";
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
	const updateVolumeReduction = useMutation(
		useCallback(
			(percent: number) => tauriAPI.updateVolumeReductionPercent(percent),
			[],
		),
	);
	const updateOverlaySize = useMutation(
		useCallback(
			(size: number) => tauriAPI.updateOverlaySizePx(size),
			[],
		),
	);

	const [localVolumeReduction, setLocalVolumeReduction] = useState<number | null>(null);
	const [localOverlaySize, setLocalOverlaySize] = useState<number | null>(null);
	const savedValue = settings?.volume_reduction_percent ?? 0;
	const volumeReductionPercent = localVolumeReduction ?? savedValue;

	const savedOverlaySize = settings?.overlay_size_px ?? 48;
	const overlaySizePx = localOverlaySize ?? savedOverlaySize;

	useEffect(() => {
		setLocalVolumeReduction(null);
	}, [savedValue]);

	useEffect(() => {
		setLocalOverlaySize(null);
	}, [savedOverlaySize]);

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
				<div style={{ marginTop: 16 }}>
					<Tooltip
						label="Not supported on this platform"
						disabled={isAudioMuteSupported !== false}
						withArrow
					>
						<div>
							<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
								<p className="settings-label" style={{ margin: 0 }}>
									Reduce volume during recording
								</p>
								<StatusIndicator status={updateVolumeReduction.status} />
							</div>
							<p className="settings-description">
								Lower system audio while dictating to keep mic input clean
							</p>
							<div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 8, maxWidth: 420 }}>
								<Slider
									value={volumeReductionPercent}
									onChange={setLocalVolumeReduction}
									onChangeEnd={(value) => updateVolumeReduction.mutate(value)}
									min={0}
									max={100}
									step={1}
									disabled={isLoading || isAudioMuteSupported === false}
									style={{ flex: 1 }}
									marks={[
										{ value: 0, label: "Off" },
										{ value: 100, label: "Mute" },
									]}
								/>
								<Text size="sm" c="dimmed" w={40} ta="right">
									{volumeReductionPercent === 0
										? "Off"
										: volumeReductionPercent === 100
											? "Mute"
											: `${volumeReductionPercent}%`}
								</Text>
							</div>
						</div>
					</Tooltip>
				</div>
				<div style={{ marginTop: 16 }}>
					<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
						<p className="settings-label" style={{ margin: 0 }}>
							Overlay size
						</p>
						<StatusIndicator status={updateOverlaySize.status} />
					</div>
					<p className="settings-description">
						Size of the floating status indicator in pixels
					</p>
					<div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 8, maxWidth: 420 }}>
						<Slider
							value={overlaySizePx}
							onChange={setLocalOverlaySize}
							onChangeEnd={(value) => updateOverlaySize.mutate(value)}
							min={24}
							max={128}
							step={1}
							disabled={isLoading}
							style={{ flex: 1 }}
							marks={[
								{ value: 24, label: "24" },
								{ value: 48, label: "48" },
								{ value: 128, label: "128" },
							]}
						/>
						<Text size="sm" c="dimmed" w={40} ta="right">
							{overlaySizePx}px
						</Text>
					</div>
				</div>
			</div>
		</div>
	);
}
