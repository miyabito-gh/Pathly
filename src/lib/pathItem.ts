export type HomeMode = 'all' | 'favorites' | 'recent' | 'frequent';
export type ItemKind = 'file' | 'folder' | 'url';

export type PathItem = {
  id: number;
  name: string;
  actualName: string;
  path: string;
  kind: ItemKind;
  extension?: string;
  tags: string[];
  category?: string | null;
  memo: string;
  favorite: boolean;
  useCount: number;
  lastUsedAt?: string;
  rawLastUsedAt?: string | null;
  excluded: boolean;
};

export const sampleItems: PathItem[] = [
  {
    id: 1,
    name: '事業計画 2026 — 最終レビュー',
    actualName: '事業計画 2026 — 最終レビュー.pdf',
    path: '\\\\fileserver01\\Shared\\Planning\\2026\\Executive Review\\事業計画 2026 — 最終レビュー.pdf',
    kind: 'file',
    extension: 'PDF',
    tags: ['企画', '経営'],
    category: '企画',
    memo: '経営会議提出用の最終版。',
    favorite: true,
    useCount: 34,
    lastUsedAt: '今日 10:32',
    excluded: false
  },
  {
    id: 2,
    name: 'デザインシステム / コンポーネント',
    actualName: 'コンポーネント',
    path: 'D:\\Workspace\\Design\\デザインシステム\\コンポーネント',
    kind: 'folder',
    tags: ['制作', 'デザイン'],
    category: '制作',
    memo: '',
    favorite: false,
    useCount: 28,
    lastUsedAt: '昨日',
    excluded: false
  },
  {
    id: 3,
    name: '顧客別パイプライン',
    actualName: '顧客別パイプライン.xlsx',
    path: '\\\\fileserver01\\Shared\\Sales\\Pipeline\\顧客別パイプライン.xlsx',
    kind: 'file',
    extension: 'XLSX',
    tags: ['営業', '顧客'],
    category: '営業',
    memo: '週次更新。',
    favorite: true,
    useCount: 19,
    lastUsedAt: '9月20日',
    excluded: false
  },
  {
    id: 4,
    name: '週報 — 9月第4週',
    actualName: '週報 — 9月第4週.docx',
    path: 'C:\\Users\\wm\\Documents\\Team\\2026\\09\\週報 — 9月第4週.docx',
    kind: 'file',
    extension: 'DOCX',
    tags: ['社内'],
    category: '社内',
    memo: '',
    favorite: false,
    useCount: 12,
    lastUsedAt: '9月19日',
    excluded: false
  },
  {
    id: 5,
    name: '契約書テンプレート',
    actualName: '契約書テンプレート.docx',
    path: '\\\\fileserver01\\Shared\\Legal\\Templates\\契約書テンプレート.docx',
    kind: 'file',
    extension: 'DOCX',
    tags: ['法務', 'テンプレート'],
    category: '法務',
    memo: '',
    favorite: false,
    useCount: 8,
    lastUsedAt: '9月17日',
    excluded: false
  }
];
