import { Kbd, NavLink, Text, Title, Tooltip } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { AlertCircle, Home, Settings } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { HistoryFeed } from "./components/HistoryFeed";
import { Logo } from "./components/Logo";
import { ApiKeySettings } from "./components/settings/ApiKeySettings";
import { AudioSettings } from "./components/settings/AudioSettings";
import { DataManagementSettings } from "./components/settings/DataManagementSettings";
import { HotkeySettings } from "./components/settings/HotkeySettings";
import { PromptSettings } from "./components/settings/PromptSettings";
import {
	DEFAULT_HOLD_HOTKEY,
	DEFAULT_PASTE_LAST_HOTKEY,
	DEFAULT_TOGGLE_HOTKEY,
} from "./lib/hotkeyDefaults";
import {
	type AppSettings,
	type HotkeyConfig,
	type ShortcutErrors,
	tauriAPI,
} from "./lib/tauri";
import { useRecordingStore } from "./stores/recordingStore";
import "./app-main.css";

type View = "home" | "settings";

function Sidebar({
	activeView,
	onViewChange,
}: {
	activeView: View;
	onViewChange: (view: View) => void;
}) {
	return (
		<aside className="sidebar">
			<header className="sidebar-header">
				<div className="sidebar-logo">
					<Logo size={32} />
				</div>
			</header>
			<nav className="sidebar-nav">
				<Tooltip label="Home" position="right" withArrow>
					<NavLink
						leftSection={<Home size={20} />}
						active={activeView === "home"}
						onClick={() => onViewChange("home")}
						variant="filled"
						className="sidebar-nav-link"
						aria-label="Navigate to Home"
					/>
				</Tooltip>
				<Tooltip label="Settings" position="right" withArrow>
					<NavLink
						leftSection={<Settings size={20} />}
						active={activeView === "settings"}
						onClick={() => onViewChange("settings")}
						variant="filled"
						className="sidebar-nav-link"
						aria-label="Navigate to Settings"
					/>
				</Tooltip>
			</nav>

			<footer className="sidebar-footer" />
		</aside>
	);
}

function HotkeyDisplay({
	config,
	error,
}: {
	config: HotkeyConfig;
	error?: string | null;
}) {
	const isDisabled = config.enabled === false;
	const parts = [
		...config.modifiers.map((m) => m.charAt(0).toUpperCase() + m.slice(1)),
		config.key,
	];

	return (
		<span
			className="kbd-combo"
			style={{
				display: "flex",
				alignItems: "center",
				gap: 6,
				opacity: isDisabled ? 0.5 : 1,
			}}
		>
			{error && (
				<Tooltip label={error} multiline w={250} withArrow position="top">
					<AlertCircle
						size={14}
						style={{ color: "var(--mantine-color-yellow-6)", flexShrink: 0 }}
					/>
				</Tooltip>
			)}
			{isDisabled && !error && (
				<span style={{ color: "var(--text-tertiary)", fontSize: 12 }}>
					(Disabled)
				</span>
			)}
			{parts.map((part, index) => (
				<span key={part}>
					<Kbd>{part}</Kbd>
					{index < parts.length - 1 && <span className="kbd-plus">+</span>}
				</span>
			))}
		</span>
	);
}

function InstructionsCard() {
	const [settings, setSettings] = useState<AppSettings | null>(null);
	const [shortcutErrors, setShortcutErrors] = useState<ShortcutErrors | null>(null);

	useEffect(() => {
		tauriAPI.getSettings().then(setSettings);
		tauriAPI.getShortcutErrors().then(setShortcutErrors);
	}, []);

	// Listen for settings changes to refresh
	useEffect(() => {
		let unlisten: (() => void) | undefined;
		tauriAPI.onSettingsChanged(() => {
			tauriAPI.getSettings().then(setSettings);
			tauriAPI.getShortcutErrors().then(setShortcutErrors);
		}).then(fn => { unlisten = fn; });
		return () => { unlisten?.(); };
	}, []);

	const toggleHotkey = settings?.toggle_hotkey ?? DEFAULT_TOGGLE_HOTKEY;
	const holdHotkey = settings?.hold_hotkey ?? DEFAULT_HOLD_HOTKEY;
	const pasteLastHotkey =
		settings?.paste_last_hotkey ?? DEFAULT_PASTE_LAST_HOTKEY;

	return (
		<div className="instructions-card animate-in">
			<h2 className="instructions-card-title">Dictate with your voice</h2>
			<div className="instructions-methods">
				<div className="instruction-method">
					<span className="instruction-label">Toggle:</span>
					<HotkeyDisplay
						config={toggleHotkey}
						error={shortcutErrors?.toggle_error}
					/>
					<span className="instruction-desc">Press to start/stop</span>
				</div>
				<div className="instruction-method">
					<span className="instruction-label">Hold:</span>
					<HotkeyDisplay
						config={holdHotkey}
						error={shortcutErrors?.hold_error}
					/>
					<span className="instruction-desc">Hold to record</span>
				</div>
				<div className="instruction-method">
					<span className="instruction-label">Paste:</span>
					<HotkeyDisplay
						config={pasteLastHotkey}
						error={shortcutErrors?.paste_last_error}
					/>
					<span className="instruction-desc">Paste last result</span>
				</div>
			</div>
			<p className="instructions-card-text">
				Speak clearly and your words will be typed wherever your cursor is. The
				overlay appears in the bottom-right corner of your screen.
			</p>
		</div>
	);
}

function HomeView() {
	return (
		<div className="main-content">
			<header className="animate-in" style={{ marginBottom: 32 }}>
				<Title order={1} mb={4}>
					Welcome to My Voice
				</Title>
				<Text c="dimmed" size="sm">
					~-~-~-~-~-~
				</Text>
			</header>

			<InstructionsCard />

			<HistoryFeed />
		</div>
	);
}

function SettingsView() {
	const status = useRecordingStore((s) => s.status);
	const isRecordingActive =
		status === "recording" || status === "processing";

	return (
		<div
			style={{
				position: "relative",
				flex: 1,
				display: "flex",
				flexDirection: "column",
				minWidth: 0,
			}}
		>
			{isRecordingActive && (
				<div
					style={{
						position: "absolute",
						inset: 0,
						zIndex: 10,
						display: "flex",
						alignItems: "center",
						justifyContent: "center",
						backgroundColor: "rgba(0, 0, 0, 0.6)",
						backdropFilter: "blur(2px)",
					}}
				>
					<Text c="dimmed" size="sm">
						Settings are locked while recording
					</Text>
				</div>
			)}
			<div className="main-content">
				<header className="animate-in" style={{ marginBottom: 32 }}>
					<Title order={1} mb={4}>
						Settings
					</Title>
					<Text c="dimmed" size="sm">
						Configure your preferences
					</Text>
				</header>

				<ApiKeySettings />
				<AudioSettings />
				<HotkeySettings />
				<PromptSettings />
				<DataManagementSettings />
			</div>
		</div>
	);
}

export default function App() {
	const [activeView, setActiveView] = useState<View>("home");
	const setRecordingStatus = useRecordingStore((s) => s.setStatus);
	const hasShownConflictNotification = useRef(false);

	// Listen for recording status changes from Rust
	useEffect(() => {
		let unlisten: (() => void) | undefined;
		tauriAPI.onRecordingStatusChanged((newStatus) => {
			setRecordingStatus(newStatus);
		}).then(fn => { unlisten = fn; });
		return () => { unlisten?.(); };
	}, [setRecordingStatus]);

	// Fetch shortcut errors for startup notification
	const [shortcutErrors, setShortcutErrors] = useState<ShortcutErrors | null>(null);

	useEffect(() => {
		tauriAPI.getShortcutErrors().then(setShortcutErrors);
	}, []);

	// Show notification on startup if any hotkeys have conflicts
	useEffect(() => {
		if (hasShownConflictNotification.current) return;
		if (!shortcutErrors) return;

		const hasErrors =
			shortcutErrors.toggle_error ||
			shortcutErrors.hold_error ||
			shortcutErrors.paste_last_error;

		if (hasErrors) {
			hasShownConflictNotification.current = true;
			notifications.show({
				title: "Hotkey Conflict Detected",
				message:
					"Some hotkeys were disabled due to conflicts. Check settings to resolve.",
				color: "yellow",
				autoClose: 5000,
			});
		}
	}, [shortcutErrors]);

	return (
		<div className="app-layout">
			<Sidebar activeView={activeView} onViewChange={setActiveView} />
			{activeView === "home" ? <HomeView /> : <SettingsView />}
		</div>
	);
}
