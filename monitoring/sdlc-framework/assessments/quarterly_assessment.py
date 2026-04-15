#!/usr/bin/env nix develop -c -- python3
"""
agentic-repos Quarterly SDLC Assessment Tool
Automates quarterly SDLC maturity assessments and improvement planning
"""

import json
import os
import sys
import logging
from datetime import datetime, timedelta
from typing import Dict, List, Optional, Tuple
from dataclasses import dataclass
import subprocess
import argparse

# Add framework path
sys.path.append(os.path.dirname(os.path.dirname(__file__)))

@dataclass
class AssessmentCriteria:
    """Assessment criteria definition"""
    name: str
    weight: float
    criteria: List[str]
    max_score: float = 100

@dataclass
class AssessmentResult:
    """Assessment result data structure"""
    category: str
    score: float
    details: Dict
    recommendations: List[str]

class SDLCAssessment:
    def __init__(self, config_path: str = None):
        self.config_path = config_path or os.path.join(os.path.dirname(__file__), '../config.json')
        self.config = self.load_config()
        self.quarter = self.get_current_quarter()
        
    def load_config(self) -> Dict:
        """Load assessment configuration"""
        try:
            with open(self.config_path, 'r') as f:
                return json.load(f)
        except FileNotFoundError:
            logging.warning(f"Config not found: {self.config_path}, using defaults")
            return self.get_default_config()
    
    def get_default_config(self) -> Dict:
        """Get default assessment configuration"""
        return {
            "assessments": {
                "quarterly": {
                    "scoring_methodology": {
                        "categories": [
                            {
                                "name": "Process Maturity",
                                "weight": 30,
                                "criteria": ["documentation", "standardization", "automation"]
                            },
                            {
                                "name": "Quality Assurance",
                                "weight": 25,
                                "criteria": ["test_coverage", "defect_rates", "quality_gates"]
                            },
                            {
                                "name": "Security Posture",
                                "weight": 25,
                                "criteria": ["vulnerability_management", "compliance", "training"]
                            },
                            {
                                "name": "Delivery Excellence",
                                "weight": 20,
                                "criteria": ["delivery_velocity", "stability", "customer_satisfaction"]
                            }
                        ]
                    }
                }
            }
        }
    
    def get_current_quarter(self) -> str:
        """Get current quarter string"""
        now = datetime.now()
        year = now.year
        quarter = (now.month - 1) // 3 + 1
        return f"{year}-Q{quarter}"
    
    def assess_process_maturity(self) -> AssessmentResult:
        """Assess process maturity category"""
        logging.info("Assessing Process Maturity...")
        
        scores = {}
        total_score = 0
        
        # Documentation assessment
        doc_score = self.assess_documentation()
        scores['documentation'] = doc_score
        total_score += doc_score
        
        # Standardization assessment
        std_score = self.assess_standardization()
        scores['standardization'] = std_score
        total_score += std_score
        
        # Automation assessment
        auto_score = self.assess_automation()
        scores['automation'] = auto_score
        total_score += auto_score
        
        # Calculate weighted score
        category_score = total_score / 3
        
        recommendations = self.get_process_maturity_recommendations(scores)
        
        return AssessmentResult(
            category="Process Maturity",
            score=category_score,
            details=scores,
            recommendations=recommendations
        )
    
    def assess_documentation(self) -> float:
        """Assess documentation quality and completeness"""
        score = 0
        max_score = 100
        
        # Check for key documentation files
        doc_files = [
            'README.md',
            'SECURITY.md',
            'TESTING.md',
            'CONTRIBUTING.md',
            'CHANGELOG.md'
        ]
        
        existing_docs = 0
        for doc in doc_files:
            if os.path.exists(doc):
                existing_docs += 1
        
        score = (existing_docs / len(doc_files)) * 40
        
        # Check API documentation
        if os.path.exists('docs/api'):
            score += 20
        
        # Check for inline documentation
        try:
            result = subprocess.run(['find', 'src', '-name', '*.rs', '-exec', 'grep', '-l', '///', '{}', '+'], 
                                  capture_output=True, text=True)
            if result.returncode == 0:
                doc_files = result.stdout.strip().split('\n')
                if len(doc_files) > 0:
                    score += 20
        except:
            pass
        
        # Check for recent documentation updates
        try:
            result = subprocess.run(['find', '.', '-name', '*.md', '-mtime', '-30'], 
                                  capture_output=True, text=True)
            if result.returncode == 0:
                recent_docs = len(result.stdout.strip().split('\n'))
                if recent_docs >= 3:
                    score += 20
        except:
            pass
        
        return min(score, max_score)
    
    def assess_standardization(self) -> float:
        """Assess process standardization"""
        score = 0
        max_score = 100
        
        # Check for consistent coding standards
        if os.path.exists('.editorconfig'):
            score += 20
        
        # Check for CI/CD standardization
        if os.path.exists('.github/workflows'):
            score += 20
        
        # Check for consistent dependency management
        if os.path.exists('Cargo.lock'):
            score += 20
        
        # Check for branch protection rules
        try:
            result = subprocess.run(['git', 'config', '--get', 'branch.master.merge'], 
                                  capture_output=True, text=True)
            if result.returncode == 0:
                score += 20
        except:
            pass
        
        # Check for standardized testing approach
        if os.path.exists('tests/') and os.path.isdir('tests/'):
            score += 20
        
        return min(score, max_score)
    
    def assess_automation(self) -> float:
        """Assess automation level"""
        score = 0
        max_score = 100
        
        # Check for automated testing
        if os.path.exists('Cargo.toml'):
            try:
                with open('Cargo.toml', 'r') as f:
                    content = f.read()
                    if '[dev-dependencies]' in content and 'tokio-test' in content:
                        score += 25
            except:
                pass
        
        # Check for CI/CD automation
        if os.path.exists('.github/workflows'):
            workflows = os.listdir('.github/workflows')
            if len(workflows) > 0:
                score += 25
        
        # Check for automated quality checks
        if os.path.exists('monitoring/sdlc-framework/quality-gates/quality-gates.sh'):
            score += 25
        
        # Check for automated deployment
        if os.path.exists('Dockerfile') or os.path.exists('docker-compose.yml'):
            score += 25
        
        return min(score, max_score)
    
    def assess_quality_assurance(self) -> AssessmentResult:
        """Assess quality assurance category"""
        logging.info("Assessing Quality Assurance...")
        
        scores = {}
        total_score = 0
        
        # Test coverage assessment
        coverage_score = self.assess_test_coverage()
        scores['test_coverage'] = coverage_score
        total_score += coverage_score
        
        # Defect rates assessment
        defect_score = self.assess_defect_rates()
        scores['defect_rates'] = defect_score
        total_score += defect_score
        
        # Quality gates assessment
        gates_score = self.assess_quality_gates()
        scores['quality_gates'] = gates_score
        total_score += gates_score
        
        # Calculate weighted score
        category_score = total_score / 3
        
        recommendations = self.get_quality_assurance_recommendations(scores)
        
        return AssessmentResult(
            category="Quality Assurance",
            score=category_score,
            details=scores,
            recommendations=recommendations
        )
    
    def assess_test_coverage(self) -> float:
        """Assess test coverage"""
        try:
            # Run cargo tarpaulin for coverage
            result = subprocess.run(['cargo', 'tarpaulin', '--out', 'Json', '--output-dir', 'target/tarpaulin'], 
                                  capture_output=True, text=True)
            
            if result.returncode == 0:
                # Parse coverage from output (simplified)
                lines = result.stdout.split('\n')
                for line in lines:
                    if '%' in line and 'coverage' in line.lower():
                        try:
                            coverage = float(line.split('%')[0].split()[-1])
                            return min(coverage, 100)
                        except:
                            continue
        except:
            pass
        
        # Fallback: check test file count vs source file count
        try:
            src_result = subprocess.run(['find', 'src', '-name', '*.rs'], capture_output=True, text=True)
            test_result = subprocess.run(['find', 'tests', '-name', '*.rs'], capture_output=True, text=True)
            
            src_files = len(src_result.stdout.strip().split('\n')) if src_result.stdout.strip() else 0
            test_files = len(test_result.stdout.strip().split('\n')) if test_result.stdout.strip() else 0
            
            if src_files > 0:
                ratio = (test_files / src_files) * 100
                return min(ratio * 20, 80)  # Scale down as this is less accurate
        except:
            pass
        
        return 0
    def assess_defect_rates(self) -> float:
        """Assess defect rates (placeholder implementation)"""
        # This would integrate with bug tracking systems
        # For now, return a score based on recent commits and test failures
        score = 80  # Baseline score
        
        # Check for recent test failures
        try:
            result = subprocess.run(['cargo', 'test'], capture_output=True, text=True)
            if result.returncode != 0:
                score -= 20
        except:
            score -= 10
        
        return max(score, 0)
    
    def assess_quality_gates(self) -> float:
        """Assess quality gates implementation"""
        score = 0
        
        # Check for quality gates script
        if os.path.exists('monitoring/sdlc-framework/quality-gates/quality-gates.sh'):
            score += 40
        
        # Check for pre-commit hooks
        if os.path.exists('.git/hooks/pre-commit'):
            score += 20
        
        # Check for CI quality gates
        if os.path.exists('.github/workflows'):
            workflows = os.listdir('.github/workflows')
            for workflow in workflows:
                if 'quality' in workflow.lower() or 'check' in workflow.lower():
                    score += 40
                    break
            else:
                score += 20  # Some CI exists
        else:
            score += 10  # Minimal credit for existing
        
        return min(score, 100)
    
    def assess_security_posture(self) -> AssessmentResult:
        """Assess security posture category"""
        logging.info("Assessing Security Posture...")
        
        scores = {}
        total_score = 0
        
        # Vulnerability management
        vuln_score = self.assess_vulnerability_management()
        scores['vulnerability_management'] = vuln_score
        total_score += vuln_score
        
        # Compliance assessment
        compliance_score = self.assess_compliance()
        scores['compliance'] = compliance_score
        total_score += compliance_score
        
        # Security training
        training_score = self.assess_security_training()
        scores['training'] = training_score
        total_score += training_score
        
        # Calculate weighted score
        category_score = total_score / 3
        
        recommendations = self.get_security_posture_recommendations(scores)
        
        return AssessmentResult(
            category="Security Posture",
            score=category_score,
            details=scores,
            recommendations=recommendations
        )
    
    def assess_vulnerability_management(self) -> float:
        """Assess vulnerability management"""
        score = 0
        
        try:
            # Run cargo audit
            result = subprocess.run(['cargo', 'audit', '--json'], capture_output=True, text=True)
            if result.returncode == 0:
                try:
                    data = json.loads(result.stdout)
                    vuln_count = data.get('vulnerabilities', {}).get('count', 0)
                    if vuln_count == 0:
                        score = 100
                    elif vuln_count <= 2:
                        score = 80
                    elif vuln_count <= 5:
                        score = 60
                    else:
                        score = 40
                except:
                    score = 70
            else:
                score = 50
        except:
            score = 30
        
        # Bonus points for regular scanning
        if os.path.exists('.github/workflows/security.yml') or os.path.exists('.github/workflows/audit.yml'):
            score = min(score + 10, 100)
        
        return score
    
    def assess_compliance(self) -> float:
        """Assess regulatory compliance"""
        score = 0
        
        # Check for security documentation
        if os.path.exists('SECURITY.md'):
            score += 25
        
        # Check for privacy policy
        if os.path.exists('PRIVACY.md'):
            score += 25
        
        # Check for compliance documentation
        if os.path.exists('COMPLIANCE.md'):
            score += 25
        
        # Check for license compliance
        if os.path.exists('LICENSE'):
            score += 25
        
        return score
    
    def assess_security_training(self) -> float:
        """Assess security training practices"""
        # Placeholder implementation
        # This would integrate with training systems
        score = 70
        
        # Check for security guidelines in documentation
        if os.path.exists('docs/security'):
            score += 20
        
        # Check for security checklists
        if os.path.exists('checklists/security.md'):
            score += 10
        
        return min(score, 100)
    
    def assess_delivery_excellence(self) -> AssessmentResult:
        """Assess delivery excellence category"""
        logging.info("Assessing Delivery Excellence...")
        
        scores = {}
        total_score = 0
        
        # Delivery velocity
        velocity_score = self.assess_delivery_velocity()
        scores['delivery_velocity'] = velocity_score
        total_score += velocity_score
        
        # Stability assessment
        stability_score = self.assess_stability()
        scores['stability'] = stability_score
        total_score += stability_score
        
        # Customer satisfaction (placeholder)
        satisfaction_score = self.assess_customer_satisfaction()
        scores['customer_satisfaction'] = satisfaction_score
        total_score += satisfaction_score
        
        # Calculate weighted score
        category_score = total_score / 3
        
        recommendations = self.get_delivery_excellence_recommendations(scores)
        
        return AssessmentResult(
            category="Delivery Excellence",
            score=category_score,
            details=scores,
            recommendations=recommendations
        )
    
    def assess_delivery_velocity(self) -> float:
        """Assess delivery velocity"""
        score = 70  # Baseline
        
        try:
            # Count commits in last 30 days
            result = subprocess.run(['git', 'log', '--since', '30 days ago', '--oneline'], 
                                  capture_output=True, text=True)
            if result.returncode == 0:
                commits = len(result.stdout.strip().split('\n'))
                if commits >= 20:
                    score = 90
                elif commits >= 10:
                    score = 80
                elif commits >= 5:
                    score = 70
                else:
                    score = 60
        except:
            pass
        
        return score
    
    def assess_stability(self) -> float:
        """Assess system stability"""
        score = 80  # Baseline
        
        # Check test pass rate
        try:
            result = subprocess.run(['cargo', 'test'], capture_output=True, text=True)
            if result.returncode == 0:
                score = 90
            else:
                score = 60
        except:
            score = 50
        
        # Check for monitoring
        if os.path.exists('monitoring/'):
            score = min(score + 10, 100)
        
        return score
    
    def assess_customer_satisfaction(self) -> float:
        """Assess customer satisfaction (placeholder)"""
        # This would integrate with customer feedback systems
        return 75
    
    def get_process_maturity_recommendations(self, scores: Dict) -> List[str]:
        """Get process maturity recommendations"""
        recommendations = []
        
        if scores.get('documentation', 0) < 70:
            recommendations.append("Improve documentation coverage and maintain regular updates")
        
        if scores.get('standardization', 0) < 70:
            recommendations.append("Standardize development processes and workflows")
        
        if scores.get('automation', 0) < 70:
            recommendations.append("Increase automation in build, test, and deployment processes")
        
        return recommendations
    
    def get_quality_assurance_recommendations(self, scores: Dict) -> List[str]:
        """Get quality assurance recommendations"""
        recommendations = []
        
        if scores.get('test_coverage', 0) < 80:
            recommendations.append("Increase test coverage to meet 80% minimum threshold")
        
        if scores.get('defect_rates', 0) < 70:
            recommendations.append("Implement stronger defect prevention and detection")
        
        if scores.get('quality_gates', 0) < 70:
            recommendations.append("Strengthen quality gates implementation and enforcement")
        
        return recommendations
    
    def get_security_posture_recommendations(self, scores: Dict) -> List[str]:
        """Get security posture recommendations"""
        recommendations = []
        
        if scores.get('vulnerability_management', 0) < 70:
            recommendations.append("Implement regular vulnerability scanning and remediation")
        
        if scores.get('compliance', 0) < 70:
            recommendations.append("Enhance compliance documentation and processes")
        
        if scores.get('training', 0) < 70:
            recommendations.append("Strengthen security training and awareness programs")
        
        return recommendations
    
    def get_delivery_excellence_recommendations(self, scores: Dict) -> List[str]:
        """Get delivery excellence recommendations"""
        recommendations = []
        
        if scores.get('delivery_velocity', 0) < 70:
            recommendations.append("Optimize delivery pipeline for faster, more reliable releases")
        
        if scores.get('stability', 0) < 70:
            recommendations.append("Improve system stability through better testing and monitoring")
        
        if scores.get('customer_satisfaction', 0) < 70:
            recommendations.append("Enhance customer feedback collection and response processes")
        
        return recommendations
    
    def calculate_overall_score(self, results: List[AssessmentResult]) -> float:
        """Calculate overall SDLC maturity score"""
        categories = self.config['assessments']['quarterly']['scoring_methodology']['categories']
        
        weighted_sum = 0
        total_weight = 0
        
        for category_config in categories:
            category_name = category_config['name']
            weight = category_config['weight']
            
            # Find the corresponding result
            category_result = next((r for r in results if r.category == category_name), None)
            if category_result:
                weighted_sum += category_result.score * weight
                total_weight += weight
        
        if total_weight > 0:
            return weighted_sum / total_weight
        return 0
    
    def generate_assessment_report(self) -> Dict:
        """Generate comprehensive assessment report"""
        logging.info(f"Generating SDLC assessment for {self.quarter}")
        
        # Assess all categories
        results = [
            self.assess_process_maturity(),
            self.assess_quality_assurance(),
            self.assess_security_posture(),
            self.assess_delivery_excellence()
        ]
        
        # Calculate overall score
        overall_score = self.calculate_overall_score(results)
        
        # Generate report
        report = {
            'quarter': self.quarter,
            'assessment_date': datetime.now().isoformat(),
            'overall_score': overall_score,
            'maturity_level': self.get_maturity_level(overall_score),
            'categories': [
                {
                    'name': result.category,
                    'score': result.score,
                    'details': result.details,
                    'recommendations': result.recommendations
                }
                for result in results
            ],
            'summary': self.generate_summary(results, overall_score),
            'improvement_plan': self.generate_improvement_plan(results)
        }
        
        return report
    
    def get_maturity_level(self, score: float) -> str:
        """Get maturity level based on score"""
        if score >= 90:
            return "Level 5: Optimizing"
        elif score >= 75:
            return "Level 4: Quantitatively Managed"
        elif score >= 60:
            return "Level 3: Defined"
        elif score >= 40:
            return "Level 2: Managed"
        else:
            return "Level 1: Initial"
    
    def generate_summary(self, results: List[AssessmentResult], overall_score: float) -> Dict:
        """Generate assessment summary"""
        strengths = []
        weaknesses = []
        
        for result in results:
            if result.score >= 80:
                strengths.append(result.category)
            elif result.score < 60:
                weaknesses.append(result.category)
        
        return {
            'overall_score': overall_score,
            'maturity_level': self.get_maturity_level(overall_score),
            'strengths': strengths,
            'weaknesses': weaknesses,
            'key_findings': self.extract_key_findings(results)
        }
    
    def extract_key_findings(self, results: List[AssessmentResult]) -> List[str]:
        """Extract key findings from assessment results"""
        findings = []
        
        for result in results:
            if result.score < 50:
                findings.append(f"Critical: {result.category} requires immediate attention ({result.score:.1f}/100)")
            elif result.score < 70:
                findings.append(f"Warning: {result.category} needs improvement ({result.score:.1f}/100)")
            elif result.score >= 90:
                findings.append(f"Excellence: {result.category} demonstrates best practices ({result.score:.1f}/100)")
        
        return findings
    
    def generate_improvement_plan(self, results: List[AssessmentResult]) -> Dict:
        """Generate prioritized improvement plan"""
        all_recommendations = []
        
        for result in results:
            for rec in result.recommendations:
                all_recommendations.append({
                    'category': result.category,
                    'priority': 'high' if result.score < 60 else 'medium' if result.score < 80 else 'low',
                    'recommendation': rec
                })
        
        # Sort by priority and impact
        all_recommendations.sort(key=lambda x: (
            0 if x['priority'] == 'high' else 1 if x['priority'] == 'medium' else 2,
            x['category']
        ))
        
        return {
            'immediate_actions': [r for r in all_recommendations if r['priority'] == 'high'],
            'short_term_goals': [r for r in all_recommendations if r['priority'] == 'medium'],
            'long_term_improvements': [r for r in all_recommendations if r['priority'] == 'low']
        }
    
    def save_report(self, report: Dict, output_path: str = None):
        """Save assessment report"""
        if not output_path:
            output_path = f"assessment-{self.quarter}.json"
        
        with open(output_path, 'w') as f:
            json.dump(report, f, indent=2)
        
        # Also save as markdown for human reading
        md_path = output_path.replace('.json', '.md')
        self.save_markdown_report(report, md_path)
        
        logging.info(f"Assessment report saved to {output_path} and {md_path}")
    
    def save_markdown_report(self, report: Dict, output_path: str):
        """Save assessment report as markdown"""
        md_content = f"""# SDLC Assessment Report - {report['quarter']}

## Executive Summary

**Overall Score:** {report['overall_score']:.1f}/100  
**Maturity Level:** {report['maturity_level']}  
**Assessment Date:** {report['assessment_date']}

### Key Findings

{chr(10).join(f"- {finding}" for finding in report['summary']['key_findings'])}

## Category Scores

"""
        
        for category in report['categories']:
            md_content += f"""### {category['name']}

**Score:** {category['score']:.1f}/100

#### Details

{chr(10).join(f"- **{k}**: {v:.1f}/100" for k, v in category['details'].items())}

#### Recommendations

{chr(10).join(f"- {rec}" for rec in category['recommendations'])}

"""
        
        md_content += f"""## Improvement Plan

### Immediate Actions (High Priority)

{chr(10).join(f"- **{action['category']}**: {action['recommendation']}" for action in report['improvement_plan']['immediate_actions'])}

### Short-term Goals (Medium Priority)

{chr(10).join(f"- **{action['category']}**: {action['recommendation']}" for action in report['improvement_plan']['short_term_goals'])}

### Long-term Improvements (Low Priority)

{chr(10).join(f"- **{action['category']}**: {action['recommendation']}" for action in report['improvement_plan']['long_term_improvements'])}

## Next Steps

1. Review findings with team leadership
2. Prioritize immediate actions based on business impact
3. Create implementation timeline for improvements
4. Schedule next assessment for {self.get_next_quarter()}

---
*Generated by agentic-repos SDLC Assessment Tool*
"""
        
        with open(output_path, 'w') as f:
            f.write(md_content)
    
    def get_next_quarter(self) -> str:
        """Get next quarter string"""
        now = datetime.now()
        year = now.year
        quarter = (now.month - 1) // 3 + 1
        
        if quarter == 4:
            year += 1
            quarter = 1
        else:
            quarter += 1
        
        return f"{year}-Q{quarter}"
    
    def run_assessment(self, output_path: str = None, interactive: bool = False) -> Dict:
        """Run complete SDLC assessment"""
        logging.info("Starting SDLC assessment...")
        
        if interactive:
            print(f"Running SDLC assessment for {self.quarter}")
            print("This may take a few minutes...\n")
        
        # Generate report
        report = self.generate_assessment_report()
        
        # Save report
        self.save_report(report, output_path)
        
        if interactive:
            print(f"\n✅ Assessment completed!")
            print(f"📊 Overall Score: {report['overall_score']:.1f}/100 ({report['maturity_level']})")
            print(f"📄 Report saved to assessment-{self.quarter}.json and assessment-{self.quarter}.md")
        
        return report

def main():
    parser = argparse.ArgumentParser(description='agentic-repos SDLC Assessment Tool')
    parser.add_argument('--quarter', help='Specify quarter (e.g., 2024-Q1)')
    parser.add_argument('--output', help='Output file path')
    parser.add_argument('--config', help='Configuration file path')
    parser.add_argument('--interactive', '-i', action='store_true', help='Interactive mode')
    
    args = parser.parse_args()
    
    # Setup logging
    logging.basicConfig(
        level=logging.INFO,
        format='%(asctime)s - %(levelname)s - %(message)s'
    )
    
    # Create assessment
    assessment = SDLCAssessment(args.config)
    
    if args.quarter:
        assessment.quarter = args.quarter
    
    # Run assessment
    report = assessment.run_assessment(args.output, args.interactive)
    
    # Return exit code based on score
    if report['overall_score'] < 60:
        sys.exit(1)  # Warning: significant improvement needed
    else:
        sys.exit(0)  # Success

if __name__ == '__main__':
    main()