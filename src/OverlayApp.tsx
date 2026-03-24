import { Loader } from "@mantine/core";
import { useResizeObserver } from "@mantine/hooks";
import { useDrag } from "@use-gesture/react";
import { AlertCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { match } from "ts-pattern";
import Logo from "./assets/logo.svg?react";
import type { RecordingStatus } from "./lib/events";
import { tauriAPI } from "./lib/tauri";
import "./overlay-global.css";

export default function OverlayApp() {
	const [status, setStatus] = useState<RecordingStatus>("idle");
	const [overlaySizePx, setOverlaySizePx] = useState(48);
	const [containerRef, rect] = useResizeObserver();
	const hasWindowDragStartedRef = useRef(false);

	const scale = overlaySizePx / 48;

	// Listen for recording status changes from Rust
	useEffect(() => {
		let unlisten: (() => void) | undefined;
		tauriAPI.onRecordingStatusChanged((newStatus) => {
			setStatus(newStatus);
		}).then(fn => { unlisten = fn; });
		return () => { unlisten?.(); };
	}, []);

	// Load overlay size from settings and listen for changes
	useEffect(() => {
		const fetchSize = () => {
			tauriAPI.getSettings().then((s) => {
				setOverlaySizePx(s.overlay_size_px ?? 48);
			});
		};
		fetchSize();

		let unlisten: (() => void) | undefined;
		tauriAPI.onSettingsChanged(() => {
			fetchSize();
		}).then(fn => { unlisten = fn; });
		return () => { unlisten?.(); };
	}, []);

	// Auto-resize overlay window to match visual (scaled) size
	useEffect(() => {
		if (rect.width > 0 && rect.height > 0) {
			tauriAPI.resizeOverlay(
				Math.ceil(rect.width * scale),
				Math.ceil(rect.height * scale),
			);
		}
	}, [rect.width, rect.height, scale]);

	// Click handler
	const handleClick = useCallback(() => {
		if (status === "idle") {
			tauriAPI.startRecording();
		} else if (status === "recording") {
			tauriAPI.stopRecording();
		}
	}, [status]);

	// Drag handler (keep existing pattern)
	const bindDrag = useDrag(({ movement: [mx, my], first, last, memo }) => {
		if (first) {
			hasWindowDragStartedRef.current = false;
			return false;
		}
		const distance = Math.sqrt(mx * mx + my * my);
		if (!memo && distance > 5) {
			hasWindowDragStartedRef.current = true;
			tauriAPI.startDragging();
			return true;
		}
		if (last) hasWindowDragStartedRef.current = false;
		return memo;
	}, { filterTaps: true });

	return (
		<div ref={containerRef} role="application" {...bindDrag()}
			style={{
				width: "fit-content", height: "fit-content",
				backgroundColor: "rgba(0, 0, 0, 0.9)",
				borderRadius: 12,
				border: "1px solid rgba(128, 128, 128, 0.9)",
				padding: 2, cursor: "grab",
				userSelect: "none", touchAction: "none",
				transform: scale !== 1 ? `scale(${scale})` : undefined,
				transformOrigin: "top left",
			}}>
			{match(status)
				.with("recording", () => (
					<button type="button" onClick={handleClick}
						style={{
							width: 48, height: 48, display: "flex",
							alignItems: "center", justifyContent: "center",
							background: "none", border: "none", cursor: "pointer",
						}}>
						<div style={{
							width: 16, height: 16, borderRadius: "50%",
							backgroundColor: "#ef4444",
							animation: "pulse 1.5s ease-in-out infinite",
						}} />
					</button>
				))
				.with("processing", () => (
					<div style={{ width: 48, height: 48, display: "flex", alignItems: "center", justifyContent: "center" }}>
						<Loader size="sm" color="white" />
					</div>
				))
				.with("error", () => (
					<button type="button" onClick={handleClick}
						style={{
							width: 48, height: 48, display: "flex",
							flexDirection: "column", alignItems: "center", justifyContent: "center",
							gap: 4, cursor: "pointer", background: "none", border: "none",
						}}>
						<AlertCircle size={20} color="#f87171" />
						<span style={{ fontSize: 9, color: "#fca5a5" }}>Try again</span>
					</button>
				))
				.with("idle", () => (
					<button type="button" onClick={handleClick}
						style={{
							width: 48, height: 48, display: "flex",
							alignItems: "center", justifyContent: "center",
							background: "none", border: "none", cursor: "pointer",
						}}>
						<Logo className="size-5" />
					</button>
				))
				.exhaustive()}
		</div>
	);
}
