<script setup lang="ts">
const hive = useHive()
const { key, keySaved, saving, models, selectedModel } = hive

const showKey = ref(false)
</script>

<template>
  <UCard>
    <div class="flex flex-col gap-5">
      <div class="flex items-center gap-3">
        <div class="rounded-lg bg-elevated p-2.5">
          <UIcon
            name="i-lucide-key-round"
            class="size-5"
          />
        </div>
        <div>
          <h2 class="text-lg font-semibold leading-tight">
            Chiave AI Hive
          </h2>
          <p class="text-sm text-muted">
            Incolla la chiave API di
            <a
              href="https://hive.requrv.ai"
              target="_blank"
              class="text-primary underline decoration-primary/40 underline-offset-2 hover:decoration-primary"
            >hive.requrv.ai</a>
            per configurare e avviare gli agenti.
          </p>
        </div>
      </div>

      <div class="flex flex-col gap-3">
        <UInput
          v-model="key"
          :type="showKey ? 'text' : 'password'"
          placeholder="Incolla qui la tua chiave Hive"
          autocomplete="off"
          spellcheck="false"
          class="font-mono"
          :trailing-icon="showKey ? 'i-lucide-eye-off' : 'i-lucide-eye'"
          :trailing-icon-click="true"
          @trailing-click="showKey = !showKey"
        />

        <div class="flex flex-wrap items-center gap-3">
          <UButton
            label="Salva chiave"
            icon="i-lucide-save"
            :loading="saving"
            :disabled="!key.trim()"
            @click="hive.saveKey"
          />
          <UButton
            v-if="keySaved"
            label="Rimuovi"
            color="error"
            variant="ghost"
            @click="hive.clearKey()"
          />
        </div>
      </div>

      <div v-if="keySaved">
        <USelect
          v-model="selectedModel"
          :items="models"
          label="Modello"
          placeholder="Seleziona un modello"
          class="w-full sm:w-96"
        />
        <p
          v-if="models.length === 0"
          class="mt-2 text-xs text-dimmed"
        >
          Nessuna voce in /v1/models: la selezione modello resterà vuota.
        </p>
      </div>
    </div>
  </UCard>
</template>
