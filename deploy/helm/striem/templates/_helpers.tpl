{{/*
Chart name, optionally overridden.
*/}}
{{- define "striem.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Fully qualified app name. Truncated to 63 chars for DNS-safe names.
*/}}
{{- define "striem.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Chart name and version, for the chart label.
*/}}
{{- define "striem.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels applied to every object.
*/}}
{{- define "striem.labels" -}}
helm.sh/chart: {{ include "striem.chart" . }}
{{ include "striem.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels shared by the whole release.
*/}}
{{- define "striem.selectorLabels" -}}
app.kubernetes.io/name: {{ include "striem.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Per-component selector labels. Call with a dict: (dict "root" . "component" "api").
*/}}
{{- define "striem.componentSelectorLabels" -}}
{{ include "striem.selectorLabels" .root }}
app.kubernetes.io/component: {{ .component }}
{{- end }}

{{/*
Per-component full labels. Call with a dict: (dict "root" . "component" "api").
*/}}
{{- define "striem.componentLabels" -}}
{{ include "striem.labels" .root }}
app.kubernetes.io/component: {{ .component }}
{{- end }}

{{/*
Service account name.
*/}}
{{- define "striem.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "striem.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Component service names. These are the in-cluster DNS names the services use to
reach each other, so the api's generated config and env point at them.
*/}}
{{- define "striem.api.fullname" -}}
{{- printf "%s-api" (include "striem.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "striem.detection.fullname" -}}
{{- printf "%s-detection" (include "striem.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "striem.vector.fullname" -}}
{{- printf "%s-vector" (include "striem.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
The name of the hostPath PersistentVolume for the detection rules. A PV belongs
to the whole cluster, so the name includes the namespace.
*/}}
{{- define "striem.detection.rulesVolumeName" -}}
{{- printf "%s-%s-rules" .Release.Namespace (include "striem.detection.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "striem.clickhouse.fullname" -}}
{{- printf "%s-clickhouse" (include "striem.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Shared image reference for the api and detection services.
*/}}
{{- define "striem.image" -}}
{{- printf "%s:%s" .Values.image.repository (.Values.image.tag | default .Chart.AppVersion) }}
{{- end }}
