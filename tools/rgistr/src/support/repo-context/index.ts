/**
 * Repo-context support module.
 *
 * Provides deterministic classification of folder roles in a repository.
 * Used to inject repo-context hints into folder synthesis prompts.
 */

export {
  RepoContextClass,
  RepoContextHint,
  RepoType,
  RepoProfile,
  PathSegmentCategory,
  PathSignals,
  ArtifactShapeSignals,
} from './types.js';

export { deriveRepoProfile } from './repoProfile.js';

export {
  extractPathSignals,
  classifyFolderContext,
} from './folderClassify.js';
