import { useState } from 'react';
import { App, Button, Dropdown, Input, Modal, Tooltip } from 'antd';
import type { MenuProps } from 'antd';
import { Bookmark, Save, Trash2 } from 'lucide-react';
import * as tauri from '../../lib/tauri';
import { usePluginStore } from '../../stores/pluginStore';
import { useTranslation } from '../../i18n';

/** Mirrors PresetManager::validate_name on the backend. */
const PRESET_NAME = /^[\p{L}\p{N} ._()-]{1,64}$/u;
const isValidName = (name: string) =>
  PRESET_NAME.test(name.trim()) && !name.includes('..') && name.trim().toLowerCase() !== 'autosave';

export default function PresetMenu({ disabled }: { disabled: boolean }) {
  const { t } = useTranslation();
  const { message, modal } = App.useApp();
  const fetchChain = usePluginStore((s) => s.fetchChain);
  const [presets, setPresets] = useState<string[]>([]);
  const [saveOpen, setSaveOpen] = useState(false);
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);

  const fail = (error: unknown) => message.error(t('chain.presetFailed', { error: String(error) }));

  const refresh = () => {
    tauri.listPresets().then(setPresets).catch(fail);
  };

  const load = (preset: string) => {
    modal.confirm({
      title: t('chain.loadPresetConfirm', { name: preset }),
      content: t('chain.loadPresetConfirmDesc'),
      onOk: async () => {
        try {
          const count = await tauri.loadPreset(preset);
          await fetchChain();
          message.success(t('chain.presetLoaded', { name: preset, count }));
        } catch (e) {
          fail(e);
        }
      },
    });
  };

  const remove = (preset: string) => {
    modal.confirm({
      title: t('chain.deletePresetConfirm', { name: preset }),
      okButtonProps: { danger: true },
      onOk: async () => {
        try {
          await tauri.deletePreset(preset);
          message.success(t('chain.presetDeleted', { name: preset }));
          refresh();
        } catch (e) {
          fail(e);
        }
      },
    });
  };

  const save = async () => {
    if (!isValidName(name)) return;
    setBusy(true);
    try {
      await tauri.savePreset(name.trim());
      message.success(t('chain.presetSaved', { name: name.trim() }));
      setSaveOpen(false);
      setName('');
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  const items: MenuProps['items'] = [
    { key: '__save', icon: <Save size={14} />, label: t('chain.savePreset'), onClick: () => setSaveOpen(true) },
    { type: 'divider' },
    ...(presets.length === 0
      ? [{ key: '__empty', disabled: true, label: t('chain.noPresets') }]
      : presets.map((preset) => ({
          key: preset,
          onClick: () => load(preset),
          label: (
            <span style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }}>
              <span>{preset}</span>
              <Button
                type="text"
                size="small"
                danger
                icon={<Trash2 size={13} />}
                aria-label={t('chain.deletePresetConfirm', { name: preset })}
                onClick={(e) => {
                  e.stopPropagation();
                  remove(preset);
                }}
              />
            </span>
          ),
        }))),
  ];

  const nameInvalid = name.length > 0 && !isValidName(name);

  return (
    <>
      <Dropdown menu={{ items }} trigger={['click']} disabled={disabled} onOpenChange={(open) => open && refresh()}>
        <Tooltip title={t('chain.presets')}>
          <Button size="middle" icon={<Bookmark size={15} />} className="btn-pill">
            {t('chain.presets')}
          </Button>
        </Tooltip>
      </Dropdown>
      <Modal
        open={saveOpen}
        title={t('chain.savePreset')}
        onOk={save}
        okButtonProps={{ disabled: !isValidName(name), loading: busy }}
        onCancel={() => setSaveOpen(false)}
        destroyOnHidden
      >
        <Input
          autoFocus
          placeholder={t('chain.presetName')}
          value={name}
          maxLength={64}
          status={nameInvalid ? 'error' : undefined}
          onChange={(e) => setName(e.target.value)}
          onPressEnter={save}
        />
        {nameInvalid && (
          <div style={{ color: 'var(--rh-error)', fontSize: 12, marginTop: 6 }}>{t('chain.presetNameInvalid')}</div>
        )}
      </Modal>
    </>
  );
}
