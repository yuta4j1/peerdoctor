'use strict'

// tests/fixtures/lockfiles/ のロックファイルを、npm に実際に作らせる。
//
// パッケージは file: で入れると、ロックファイルの resolved が file: になり、peerdoctor が
// 未対応の構成として拒否してしまう。そこで、手で定義したパッケージを返すレジストリを
// ローカルに立て、本物のレジストリから入れたのと同じ形のロックファイルを作る。

const crypto = require('crypto')
const fs = require('fs')
const http = require('http')
const os = require('os')
const path = require('path')
const { promisify } = require('util')
const execFile = promisify(require('child_process').execFile)

// ロックファイルの resolved に入るので、毎回同じ内容になるよう固定する
const PORT = 4873
const REGISTRY = `http://localhost:${PORT}`
const OUT_DIR = path.join(__dirname, '..', '..', 'tests', 'fixtures', 'lockfiles')

const PACKAGES = {
  next: {
    '14.2.0': { peerDependencies: { react: '^18' } },
    '15.3.0': { peerDependencies: { react: '^18 || ^19' } },
    '16.3.0': { peerDependencies: { react: '^19' } },
  },
  react: {
    '18.2.0': {},
    '19.0.0': {},
  },
  'plugin-a': {
    '2.1.0': { peerDependencies: { next: '^14 || ^15' } },
  },
  'plugin-b': {
    '3.0.0': { peerDependencies: { next: '>=15 <17' } },
  },
  'plugin-c': {
    '1.0.0': {
      peerDependencies: { next: '^15', react: '^18' },
      peerDependenciesMeta: { react: { optional: true } },
    },
  },
  'legacy-host': {
    '1.0.0': { dependencies: { next: '14.2.0', 'plugin-old': '1.0.0' } },
  },
  'plugin-old': {
    '1.0.0': { peerDependencies: { next: '^14' } },
  },
  '@acme/next-plugin': {
    '1.0.0': { peerDependencies: { next: '^15' } },
  },
}

const SCENARIOS = {
  basic: {
    lockfileVersion: 3,
    dependencies: {
      next: '15.3.0',
      react: '18.2.0',
      'plugin-a': '2.1.0',
      'plugin-b': '3.0.0',
      'plugin-c': '1.0.0',
    },
  },
  'basic-v2': {
    lockfileVersion: 2,
    dependencies: {
      next: '15.3.0',
      react: '18.2.0',
      'plugin-a': '2.1.0',
      'plugin-b': '3.0.0',
      'plugin-c': '1.0.0',
    },
  },
  nested: {
    lockfileVersion: 3,
    dependencies: {
      next: '15.3.0',
      react: '18.2.0',
      'plugin-a': '2.1.0',
      'legacy-host': '1.0.0',
    },
  },
  scoped: {
    lockfileVersion: 3,
    dependencies: {
      next: '15.3.0',
      react: '18.2.0',
      '@acme/next-plugin': '1.0.0',
    },
  },
}

const tarballFileName = (name, version) => `${name.split('/').pop()}-${version}.tgz`
const tarballUrl = (name, version) => `${REGISTRY}/${name}/-/${tarballFileName(name, version)}`

async function packAll (workDir) {
  const tarballs = new Map()
  for (const [name, versions] of Object.entries(PACKAGES)) {
    for (const [version, manifest] of Object.entries(versions)) {
      const packageDir = path.join(workDir, 'packages', name, version)
      fs.mkdirSync(packageDir, { recursive: true })
      fs.writeFileSync(
        path.join(packageDir, 'package.json'),
        JSON.stringify({ name, version, ...manifest }, null, 2) + '\n')
      const { stdout } = await execFile('npm', ['pack', '--json', '--pack-destination', workDir], {
        cwd: packageDir,
      })
      const [{ filename }] = JSON.parse(stdout)
      const data = fs.readFileSync(path.join(workDir, filename))
      tarballs.set(`${name}@${version}`, {
        data,
        integrity: 'sha512-' + crypto.createHash('sha512').update(data).digest('base64'),
        shasum: crypto.createHash('sha1').update(data).digest('hex'),
      })
    }
  }
  return tarballs
}

function packument (name, tarballs) {
  const versions = Object.entries(PACKAGES[name])
  return {
    name,
    'dist-tags': { latest: versions[versions.length - 1][0] },
    modified: '2026-01-01T00:00:00.000Z',
    versions: Object.fromEntries(versions.map(([version, manifest]) => {
      const { integrity, shasum } = tarballs.get(`${name}@${version}`)
      return [version, {
        name,
        version,
        ...manifest,
        dist: { tarball: tarballUrl(name, version), integrity, shasum },
      }]
    })),
  }
}

function startRegistry (tarballs) {
  const server = http.createServer((req, res) => {
    const url = decodeURIComponent(req.url.split('?')[0]).slice(1)
    const tarball = url.match(/^(.+)\/-\/[^/]+-(\d[^/]*)\.tgz$/)
    if (tarball && tarballs.has(`${tarball[1]}@${tarball[2]}`)) {
      res.writeHead(200, { 'content-type': 'application/octet-stream' })
      res.end(tarballs.get(`${tarball[1]}@${tarball[2]}`).data)
    } else if (PACKAGES[url]) {
      res.writeHead(200, { 'content-type': 'application/json' })
      res.end(JSON.stringify(packument(url, tarballs)))
    } else {
      res.writeHead(404, { 'content-type': 'application/json' })
      res.end('{"error":"not found"}')
    }
  })
  return new Promise((resolve) => server.listen(PORT, () => resolve(server)))
}

async function generate (workDir, scenarioName, { lockfileVersion, dependencies }) {
  const projectDir = path.join(workDir, 'projects', scenarioName)
  fs.mkdirSync(projectDir, { recursive: true })
  const packageJson = { name: `fixture-${scenarioName}`, version: '0.0.0', private: true, dependencies }
  fs.writeFileSync(path.join(projectDir, 'package.json'), JSON.stringify(packageJson, null, 2) + '\n')

  await execFile('npm', [
    'install',
    '--package-lock-only',
    `--lockfile-version=${lockfileVersion}`,
    `--registry=${REGISTRY}`,
    `--cache=${path.join(workDir, 'cache')}`,
    '--no-audit',
    '--no-fund',
    '--ignore-scripts',
  ], { cwd: projectDir })

  const outDir = path.join(OUT_DIR, scenarioName)
  fs.mkdirSync(outDir, { recursive: true })
  for (const file of ['package.json', 'package-lock.json']) {
    fs.copyFileSync(path.join(projectDir, file), path.join(outDir, file))
  }
  console.log(`wrote ${scenarioName}/ (lockfileVersion ${lockfileVersion})`)
}

async function main () {
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'peerdoctor-lockfiles-'))
  const tarballs = await packAll(workDir)
  const server = await startRegistry(tarballs)
  try {
    for (const [name, scenario] of Object.entries(SCENARIOS)) {
      await generate(workDir, name, scenario)
    }
  } finally {
    server.close()
    fs.rmSync(workDir, { recursive: true, force: true })
  }
}

main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
