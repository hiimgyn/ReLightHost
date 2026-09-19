import { useEffect, useRef, useState } from 'react';
import type { InputRef } from 'antd';
import type { MessageInstance } from 'antd/es/message/interface';
import * as tauri from '../../lib/tauri';
import { useTranslation } from '../../i18n';

interface UsePluginRenameOptions {
  instanceId: string;
  pluginName: string;
  interactionLocked: boolean;
  messageApi: MessageInstance;
}

/** Inline rename state machine for a plugin card's name field. */
export function usePluginRename({ instanceId, pluginName, interactionLocked, messageApi }: UsePluginRenameOptions) {
  const { t } = useTranslation();
  const [isRenaming, setIsRenaming] = useState(false);
  const [isRenamingBusy, setIsRenamingBusy] = useState(false);
  const [editName, setEditName] = useState(pluginName);
  const renameInputRef = useRef<InputRef | null>(null);

  // Sync edit name when plugin name changes externally
  useEffect(() => {
    if (!isRenaming) setEditName(pluginName);
  }, [pluginName, isRenaming]);

  useEffect(() => {
    if (isRenaming) {
      setTimeout(() => renameInputRef.current?.focus(), 0);
    }
  }, [isRenaming]);

  const startRenaming = () => {
    if (interactionLocked) return;
    setEditName(pluginName);
    setIsRenaming(true);
  };

  const confirmRename = async () => {
    if (interactionLocked || isRenamingBusy) return;
    const trimmed = editName.trim();
    if (trimmed !== pluginName) {
      try {
        setIsRenamingBusy(true);
        console.debug('PluginCard: rename confirm', { instanceId, from: pluginName, to: trimmed });
        await tauri.renamePlugin(instanceId, trimmed);
        if (!trimmed) {
          messageApi.success(t('card.renameSuccess'));
        }
      } catch (err) {
        messageApi.error(t('card.renameFailed', { error: String(err) }));
        setEditName(pluginName);
      } finally {
        setIsRenamingBusy(false);
      }
    }
    setIsRenaming(false);
  };

  const cancelRename = () => {
    if (isRenamingBusy) return;
    console.debug('PluginCard: rename cancelled', { instanceId, name: pluginName });
    setEditName(pluginName);
    setIsRenaming(false);
  };

  return {
    isRenaming,
    isRenamingBusy,
    editName,
    setEditName,
    renameInputRef,
    startRenaming,
    confirmRename,
    cancelRename,
  };
}
