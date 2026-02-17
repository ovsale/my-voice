import { create } from "zustand";
import type { RecordingStatus } from "../lib/events";

interface RecordingState {
	status: RecordingStatus;
	setStatus: (status: RecordingStatus) => void;
}

export const useRecordingStore = create<RecordingState>((set) => ({
	status: "idle",
	setStatus: (status) => set({ status }),
}));
