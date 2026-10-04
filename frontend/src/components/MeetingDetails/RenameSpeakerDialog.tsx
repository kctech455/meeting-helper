"use client";

import { useEffect, useState } from 'react';
import { Loader2, Users, AlertCircle, CheckCircle2 } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';

interface RenameSpeakerDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Current speaker labels present in this meeting (for the "from" picker). */
  speakerLabels: string[];
  onRename: (from: string, to: string) => Promise<number | null>;
}

/**
 * Batch-renames a speaker label across a whole meeting.
 * Overwrites every segment labelled `from` to `to` (matches the one-at-a-time
 * rename behavior). "from" is chosen from the existing labels in this meeting.
 */
export function RenameSpeakerDialog({
  open,
  onOpenChange,
  speakerLabels,
  onRename,
}: RenameSpeakerDialogProps) {
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset state only on closed -> open transition.
  useEffect(() => {
    if (open) {
      setFrom('');
      setTo('');
      setError(null);
      setIsSaving(false);
    }
  }, [open]);

  const canSubmit =
    from.trim().length > 0 &&
    to.trim().length > 0 &&
    !isSaving;

  const handleRename = async () => {
    if (!canSubmit) return;
    setIsSaving(true);
    setError(null);
    try {
      const count = await onRename(from, to);
      if (count === null) {
        setError('Rename failed. Check the console for details.');
        return;
      }
      await Analytics.track('rename_speaker', {
        meeting_speakers: String(speakerLabels.length),
        updated_count: String(count),
      });
      toast.success(`Renamed "${from}" to "${to}" (${count} segments)`);
      onOpenChange(false);
    } catch (err: any) {
      const msg = typeof err === 'string' ? err : (err?.message || String(err));
      setError(msg);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[420px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Users className="h-5 w-5 text-blue-600" />
            Rename Speaker
          </DialogTitle>
          <DialogDescription>
            Replace every occurrence of a speaker label in this meeting. Segments
            labelled with the old name are overwritten — same as renaming one chip.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <label className="text-sm font-medium text-gray-700">Current label</label>
            <select
              value={from}
              onChange={(e) => setFrom(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
            >
              <option value="" disabled>
                {speakerLabels.length ? 'Select a speaker…' : 'No speakers in this meeting'}
              </option>
              {speakerLabels.map((label) => (
                <option key={label} value={label}>
                  {label}
                </option>
              ))}
            </select>
            {speakerLabels.length === 0 && (
              <p className="text-xs text-muted-foreground">
                No speaker labels found. Run diarization or assign a speaker first.
              </p>
            )}
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium text-gray-700">New name</label>
            <input
              value={to}
              onChange={(e) => setTo(e.target.value)}
              placeholder="e.g. John"
              autoFocus
              className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
              onKeyDown={(e) => {
                if (e.key === 'Enter' && canSubmit) void handleRename();
              }}
            />
          </div>

          {isSaving && (
            <div className="flex items-center gap-2 text-sm text-blue-600">
              <Loader2 className="h-4 w-4 animate-spin" />
              Renaming…
            </div>
          )}
          {error && (
            <div className="flex items-start gap-2 bg-red-50 border border-red-200 rounded-lg p-3">
              <AlertCircle className="h-4 w-4 text-red-600 flex-shrink-0 mt-0.5" />
              <p className="text-sm text-red-800">{error}</p>
            </div>
          )}
          {!isSaving && !error && from && to && (
            <div className="flex items-start gap-2 text-xs text-muted-foreground">
              <CheckCircle2 className="h-4 w-4 text-green-600 flex-shrink-0" />
              <span>
                Will overwrite every segment labelled{' '}
                <span className="font-medium text-gray-700">{from}</span>{' '}
                →{' '}
                <span className="font-medium text-gray-700">{to}</span>
                .
              </span>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isSaving}>
            Cancel
          </Button>
          <Button
            onClick={() => void handleRename()}
            disabled={!canSubmit}
            className="bg-blue-600 hover:bg-blue-700"
          >
            Rename
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
