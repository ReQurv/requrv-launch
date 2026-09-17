import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'

export type ServiceId = 'opencode' | 'codex'
export type LaunchMode = 'app' | 'terminal'

export interface ServiceStatus {
  opencode: boolean
  codex: boolean
  opencode_app: boolean
  opencode_cli: boolean
  codex_app: boolean
  codex_cli: boolean
  codex_app_configured: boolean
}

export interface ChatgptAppResult {
  restart_required: boolean
}

export interface HiveModel {
  id: string
  model_type: string
}

export const SERVICE_META: Record<
  ServiceId,
  {
    title: string
    description: string
    icon: string
    downloadUrl: string
  }
> = {
  opencode: {
    title: 'OpenCode',
    description: 'IDE di coding di OpenCode. Scarica e installa l\'app, poi riprova.',
    icon: 'i-simple-icons-opencode',
    downloadUrl: 'https://opencode.ai/download'
  },
  codex: {
    title: 'Codex',
    description: 'CLI di coding di OpenAI. Usa un profilo dedicato puntato ad AI Hive.',
    icon: 'i-simple-icons-openai',
    downloadUrl: 'https://chatgpt.com/codex'
  }
}

const isTauri = computed(() => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window)

const key = ref('')
const keySaved = ref(false)
const saving = ref(false)
const models = ref<HiveModel[]>([])
const selectedModel = ref('')

// Only TEXT_GENERATION models make sense as an LLM; fall back to the full
// list if the gateway ever stops reporting the type.
const chatModels = computed(() => {
  const text = models.value.filter(m => m.model_type === 'TEXT_GENERATION')
  return text.length > 0 ? text : models.value
})
const chatModelIds = computed(() => chatModels.value.map(m => m.id))
const status = ref<ServiceStatus | null>(null)
const refreshing = ref(false)
const launching = ref<ServiceId | null>(null)
const keyModalOpen = ref(false)
const launchTarget = ref<ServiceId | null>(null)
const launchModalOpen = ref(false)
const restartModalOpen = ref(false)
const restarting = ref(false)
const restoring = ref(false)

export function useHive() {
  const toast = useToast()

  async function refreshStatus() {
    if (!isTauri.value || refreshing.value) return
    refreshing.value = true
    try {
      status.value = await invoke<ServiceStatus>('check_services')
    } catch {
      // stato non determinante: mantieni i valori attuali
    } finally {
      refreshing.value = false
    }
  }

  async function loadSavedKey() {
    if (!isTauri.value) return
    try {
      const saved = await invoke<string | null>('get_hive_key')
      if (saved) {
        key.value = saved
        keySaved.value = true
        await loadModels()
      }
    } catch {
      // chiave assente o non leggibile: ignora
    }
  }

  async function loadModels() {
    if (!key.value.trim()) return
    try {
      models.value = await invoke<HiveModel[]>('list_hive_models', {
        key: key.value.trim()
      })
      const first = chatModelIds.value[0]
      if (first && !chatModelIds.value.includes(selectedModel.value)) {
        selectedModel.value = first
      }
    } catch (error) {
      toast.add({
        title: 'Impossibile caricare i modelli da AI Hive',
        description: String(error),
        color: 'error'
      })
    }
  }

  async function saveKey(): Promise<boolean> {
    const trimmed = key.value.trim()
    if (!trimmed || saving.value) return false
    saving.value = true
    try {
      await invoke('set_hive_key', { key: trimmed })
      key.value = trimmed
      keySaved.value = true
      await loadModels()
      toast.add({
        title: 'Chiave salvata e verificata',
        description: `${models.value.length} modelli disponibili su AI Hive.`,
        color: 'success'
      })
      return true
    } catch (error) {
      toast.add({
        title: 'Salvataggio non riuscito',
        description: String(error),
        color: 'error'
      })
      return false
    } finally {
      saving.value = false
    }
  }

  async function clearKey() {
    if (isTauri.value) {
      try {
        await invoke('delete_hive_key')
      } catch {
        // il file potrebbe già non esistere
      }
    }
    key.value = ''
    keySaved.value = false
    models.value = []
    selectedModel.value = ''
  }

  // La destinazione (app o terminale) è scelta dall'utente: se entrambe sono
  // disponibili si apre il modale, altrimenti si avvia quella esistente.
  async function requestLaunch(service: ServiceId) {
    if (!key.value.trim() || !selectedModel.value || launching.value) return
    await refreshStatus()
    const appAvailable = status.value?.[`${service}_app`] ?? false
    const terminalAvailable = status.value?.[`${service}_cli`] ?? false
    if (appAvailable && terminalAvailable) {
      launchTarget.value = service
      launchModalOpen.value = true
    } else if (terminalAvailable) {
      await launch(service, 'terminal')
    } else if (appAvailable) {
      await launch(service, 'app')
    }
  }

  async function launch(service: ServiceId, mode: LaunchMode) {
    if (!key.value.trim() || !selectedModel.value || launching.value) return
    launching.value = service
    try {
      if (service === 'codex' && mode === 'app') {
        // Il flusso app configura ChatGPT su AI Hive e, se l'app è già aperta,
        // chiede di riavviarla perché il catalogo modelli si legge all'avvio.
        const result = await invoke<ChatgptAppResult>('configure_chatgpt_app', {
          model: selectedModel.value,
          key: key.value.trim(),
          models: chatModels.value
        })
        toast.add({
          title: 'ChatGPT configurato su AI Hive',
          color: 'success'
        })
        if (result.restart_required) {
          restartModalOpen.value = true
        } else {
          await invoke('open_chatgpt_app')
        }
        await refreshStatus()
        return
      }
      await invoke('launch_service', {
        service,
        model: selectedModel.value,
        key: key.value.trim(),
        mode
      })
      toast.add({
        title: `${SERVICE_META[service].title} avviato`,

        color: 'success'
      })
      await refreshStatus()
    } catch (error) {
      toast.add({
        title: `Avvio di ${SERVICE_META[service].title} non riuscito`,
        description: String(error),
        color: 'error'
      })
    } finally {
      launching.value = null
    }
  }

  async function confirmChatgptRestart() {
    restartModalOpen.value = false
    restarting.value = true
    try {
      await invoke('restart_chatgpt_app')
      toast.add({
        title: 'ChatGPT riavviato con AI Hive',
        color: 'success'
      })
    } catch (error) {
      toast.add({
        title: 'Riavvio di ChatGPT non riuscito',
        description: String(error),
        color: 'error'
      })
    } finally {
      restarting.value = false
      await refreshStatus()
    }
  }

  function cancelChatgptRestart() {
    restartModalOpen.value = false
    toast.add({
      title: 'ChatGPT si aggiornerà al prossimo avvio',
      color: 'info'
    })
  }

  async function restoreChatgpt() {
    if (!isTauri.value || restoring.value) return
    restoring.value = true
    try {
      const result = await invoke<ChatgptAppResult>('restore_chatgpt_app')
      toast.add({
        title: 'ChatGPT ripristinato',
        color: 'success'
      })
      if (result.restart_required) {
        restartModalOpen.value = true
      } else {
        await refreshStatus()
      }
    } catch (error) {
      toast.add({
        title: 'Ripristino di ChatGPT non riuscito',
        description: String(error),
        color: 'error'
      })
    } finally {
      restoring.value = false
    }
  }

  async function openExternal(url: string) {
    if (isTauri.value) {
      await openUrl(url)
    } else {
      window.open(url, '_blank', 'noopener,noreferrer')
    }
  }

  return {
    isTauri,
    key,
    keySaved,
    saving,
    models,
    selectedModel,
    status,
    refreshing,
    launching,
    keyModalOpen,
    launchTarget,
    launchModalOpen,
    restartModalOpen,
    restarting,
    restoring,
    chatModels,
    chatModelIds,
    refreshStatus,
    loadSavedKey,
    saveKey,
    clearKey,
    requestLaunch,
    launch,
    confirmChatgptRestart,
    cancelChatgptRestart,
    restoreChatgpt,
    openExternal
  }
}
