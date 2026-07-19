import { create } from "zustand";

interface ToastState {
  message: string | null;
  show: (message: string) => void;
  clear: () => void;
}

let timer: ReturnType<typeof setTimeout> | null = null;

export const useToast = create<ToastState>((set) => ({
  message: null,
  show: (message) => {
    set({ message });
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => set({ message: null }), 2200);
  },
  clear: () => set({ message: null }),
}));
