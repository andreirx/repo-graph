/**
 * Repository registration entity.
 */
export interface Repo {
	repoUid: string;
	name: string;
	rootPath: string;
	defaultBranch: string | null;
	createdAt: string;
	metadataJson: string | null;
}
