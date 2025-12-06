
export interface SigmaRule {
  id: string;
  enabled: boolean;
  title?: string;
  level?: string;
  category?: string;
  product?: string;
  service?: string;
  author?: string;
  description?: string;
  tags?: string[];
  logsource?: {
    product?: string;
    service?: string;
    category?: string;
  };
}
