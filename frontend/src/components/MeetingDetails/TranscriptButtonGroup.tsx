"use client";

import { useState, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, FolderOpen, RefreshCw, Users, Mic, Loader2 } from 'lucide-react';
import { toast } from 'sonner';
import { invoke } from '@tauri-apps/api/core';
import Analytics from '@/lib/analytics';
import { RetranscribeDialog } from './RetranscribeDialog';
import { RenameSpeakerDialog } from './RenameSpeakerDialog';
import { useConfig } from '@/contexts/ConfigContext';


interface TranscriptButtonGroupProps {
  transcriptCount: number;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
  /** Batch-rename a speaker label across the meeting (from -> to). */
  onRenameSpeaker?: (from: string, to: string) => Promise<number | null>;
  /** Distinct speaker labels present in this meeting (for the picker). */
  speakerLabels?: string[];
}


export function TranscriptButtonGroup({
  transcriptCount,
  onCopyTranscript,
  onOpenMeetingFolder,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
  onRenameSpeaker,
  speakerLabels = [],
}: TranscriptButtonGroupProps) {
  const { betaFeatures } = useConfig();
  const [showRetranscribeDialog, setShowRetranscribeDialog] = useState(false);
  const [showRenameSpeakerDialog, setShowRenameSpeakerDialog] = useState(false);

  const handleRetranscribeComplete = useCallback(async () => {
    // Refetch transcripts to show the updated data
    if (onRefetchTranscripts) {
      await onRefetchTranscripts();
    }
  }, [onRefetchTranscripts]);

  // Manual speaker-diarization trigger (same command the recorder auto-fires post-save).
  const [isDiarizing, setIsDiarizing] = useState(false);
  const handleRunDiarization = useCallback(async () => {
    if (!meetingId || !meetingFolderPath || isDiarizing) return;
    const audioPath = `${meetingFolderPath}/audio.mp4`;
    setIsDiarizing(true);
    Analytics.trackButtonClick('run_diarization', 'meeting_details');
    toast.info('Speaker diarization started…');
    try {
      const res: string = await invoke('start_diarization', { audioPath, meetingId });
      toast.success(res || 'Speaker labels applied');
      await onRefetchTranscripts?.();
    } catch (err) {
      console.error('Diarization failed:', err);
      toast.error(`Diarization failed: ${err}`);
      // The auto-trigger is silent on failure — surface it here so it's visible.
    } finally {
      setIsDiarizing(false);
    }
  }, [meetingId, meetingFolderPath, isDiarizing, onRefetchTranscripts]);

  return (
    <div className="flex items-center justify-center w-full gap-2">
      <ButtonGroup>
        <Button
          variant="outline"
          size="sm"
          className="px-2 @[22rem]:px-3"
          onClick={() => {
            Analytics.trackButtonClick('copy_transcript', 'meeting_details');
            onCopyTranscript();
          }}
          disabled={transcriptCount === 0}
          title={transcriptCount === 0 ? 'No transcript available' : 'Copy Transcript'}
        >
          <Copy />
          <span className="hidden @[22rem]:inline">Copy</span>
        </Button>

        <Button
          size="sm"
          variant="outline"
          className="px-2 @[22rem]:px-4"
          onClick={() => {
            Analytics.trackButtonClick('open_recording_folder', 'meeting_details');
            onOpenMeetingFolder();
          }}
          title="Open Recording Folder"
        >
          <FolderOpen className="@[22rem]:mr-2" size={18} />
          <span className="hidden @[22rem]:inline">Recording</span>
        </Button>

        {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="bg-gradient-to-r from-blue-50 to-purple-50 hover:from-blue-100 hover:to-purple-100 border-blue-200 px-2 @[22rem]:px-4"
            onClick={() => {
              Analytics.trackButtonClick('enhance_transcript', 'meeting_details');
              setShowRetranscribeDialog(true);
            }}
            title="Retranscribe to enhance your recorded audio"
          >
            <RefreshCw className="@[22rem]:mr-2" size={18} />
            <span className="hidden @[22rem]:inline">Enhance</span>
          </Button>
        )}

        {onRenameSpeaker && (
          <Button
            size="sm"
            variant="outline"
            className="px-2 @[22rem]:px-4"
            onClick={() => {
              Analytics.trackButtonClick('rename_speaker', 'meeting_details');
              setShowRenameSpeakerDialog(true);
            }}
            disabled={speakerLabels.length === 0}
            title={
              speakerLabels.length === 0
                ? 'No speaker labels to rename'
                : 'Rename a speaker across the whole meeting'
            }
          >
            <Users className="@[22rem]:mr-2" size={18} />
            <span className="hidden @[22rem]:inline">Relabel</span>
          </Button>
        )}

        {meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="px-2 @[22rem]:px-4"
            onClick={() => void handleRunDiarization()}
            disabled={isDiarizing}
            title={isDiarizing ? 'Diarizing…' : 'Run speaker diarization on this recording'}
          >
            {isDiarizing ? (
              <Loader2 className="@[22rem]:mr-2 animate-spin" size={18} />
            ) : (
              <Mic className="@[22rem]:mr-2" size={18} />
            )}
            <span className="hidden @[22rem]:inline">{isDiarizing ? 'Diarizing…' : 'Diarize'}</span>
          </Button>
        )}
      </ButtonGroup>

      {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
        <RetranscribeDialog
          open={showRetranscribeDialog}
          onOpenChange={setShowRetranscribeDialog}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onComplete={handleRetranscribeComplete}
        />
      )}

      {onRenameSpeaker && (
        <RenameSpeakerDialog
          open={showRenameSpeakerDialog}
          onOpenChange={setShowRenameSpeakerDialog}
          speakerLabels={speakerLabels}
          onRename={onRenameSpeaker}
        />
      )}
    </div>
  );
}
