import { KeyRound } from "lucide-react";
import { useKeychainConsent } from "@/keychainConsent";
import { Button } from "./ui/button";

/** One-time explainer shown before Trawl first reads/writes the macOS Keychain,
 *  so the native permission prompt that follows isn't a surprise. Mounted once. */
export function KeychainConsentModal() {
  const open = useKeychainConsent((s) => s.open);
  const confirm = useKeychainConsent((s) => s.confirm);
  const cancel = useKeychainConsent((s) => s.cancel);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-6" onClick={cancel}>
      <div
        className="w-[460px] rounded-lg border border-border bg-background p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 flex items-center gap-2">
          <KeyRound className="size-4 text-primary" />
          <h2 className="text-base font-semibold">Access to your macOS Keychain</h2>
        </div>
        <div className="space-y-2 text-sm text-muted-foreground">
          <p>
            Trawl keeps git-host tokens and rule secrets in the <strong>macOS Keychain</strong> rather than
            in plain files on disk.
          </p>
          <p>
            To store or read this value, macOS will now ask your permission to access Trawl's own Keychain
            items. Trawl can only touch the entries it created — it never sees anything else in your Keychain.
          </p>
          <p>
            Tip: choose <strong>Always Allow</strong> in the macOS prompt to avoid being asked again.
          </p>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={cancel}>
            Cancel
          </Button>
          <Button size="sm" onClick={confirm}>
            Continue
          </Button>
        </div>
      </div>
    </div>
  );
}
